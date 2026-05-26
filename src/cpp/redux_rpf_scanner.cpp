#include <cctype>
#include <cstdio>
#include <filesystem>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

#ifdef _WIN32
#include <windows.h>
#else
#include <sys/wait.h>
#include <unistd.h>
#endif

namespace fs = std::filesystem;

static constexpr const char* SCANNER_NAME = "redux_rpf_scanner";
static constexpr const char* SCANNER_VERSION = "0.2.0";

static std::string path_to_utf8(const fs::path& p) {
    auto u8 = p.u8string();
    return std::string(u8.begin(), u8.end());
}

static std::string trim_copy(const std::string& text) {
    std::size_t start = 0;
    while (start < text.size() && std::isspace(static_cast<unsigned char>(text[start]))) {
        ++start;
    }
    std::size_t end = text.size();
    while (end > start && std::isspace(static_cast<unsigned char>(text[end - 1]))) {
        --end;
    }
    return text.substr(start, end - start);
}

static std::string json_escape(const std::string& text) {
    std::string out;
    out.reserve(text.size() + 8);
    for (unsigned char ch : text) {
        switch (ch) {
        case '\"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\b': out += "\\b"; break;
        case '\f': out += "\\f"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (ch < 0x20) {
                char buf[7];
                std::snprintf(buf, sizeof(buf), "\\u%04x", ch);
                out += buf;
            } else {
                out.push_back(static_cast<char>(ch));
            }
        }
    }
    return out;
}

static std::string json_string(const std::string& text) {
    return "\"" + json_escape(text) + "\"";
}

#ifdef _WIN32
static std::wstring utf8_to_wide(const std::string& text) {
    if (text.empty()) {
        return L"";
    }

    int needed = MultiByteToWideChar(CP_UTF8, 0, text.c_str(), -1, nullptr, 0);
    if (needed <= 0) {
        throw std::runtime_error("Failed to convert UTF-8 to UTF-16.");
    }

    std::wstring result(static_cast<std::size_t>(needed - 1), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, text.c_str(), -1, result.data(), needed);
    return result;
}

// Correct Windows command-line quoting.
// This is needed because CreateProcess receives one command-line string,
// and Windows paths like "Program Files (x86)" must survive parsing.
static std::wstring quote_windows_arg(const std::wstring& arg) {
    if (arg.empty()) {
        return L"\"\"";
    }

    bool needs_quotes = false;
    for (wchar_t ch : arg) {
        if (ch == L' ' || ch == L'\t' || ch == L'\n' || ch == L'\v' || ch == L'"') {
            needs_quotes = true;
            break;
        }
    }

    if (!needs_quotes) {
        return arg;
    }

    std::wstring out;
    out.push_back(L'"');

    std::size_t backslashes = 0;

    for (wchar_t ch : arg) {
        if (ch == L'\\') {
            ++backslashes;
        } else if (ch == L'"') {
            out.append(backslashes * 2 + 1, L'\\');
            out.push_back(L'"');
            backslashes = 0;
        } else {
            out.append(backslashes, L'\\');
            backslashes = 0;
            out.push_back(ch);
        }
    }

    out.append(backslashes * 2, L'\\');
    out.push_back(L'"');

    return out;
}
#endif

enum class DeprecatedModeFlag {
    None,
    All,
    TargetsOnly
};

struct Args {
    std::string command;
    fs::path clean;
    fs::path modded;
    fs::path archive;
    fs::path keys;
    fs::path out;
    fs::path backend;
    int depth = 2;
    std::string mode;
    bool mode_set = false;
    DeprecatedModeFlag deprecated_mode = DeprecatedModeFlag::None;
    fs::path component_rules;
    fs::path target_rules;
    fs::path rules_dir;
};

static void usage() {
    std::cout <<
R"(Redux RPF Component Scanner - C++ frontend + rpf-rs backend

Commands:
  compare-rpf --clean <clean.update.rpf> --modded <modded.update.rpf> --keys <keys_dir> --out <report.json> [--backend <rpf_backend_rs.exe>] [--depth 2] [--mode fast|targeted|deep|full] [--all|--targets-only]
  scan-rpf    --archive <update.rpf> --keys <keys_dir> --out <manifest.json> [--backend <rpf_backend_rs.exe>] [--depth 2] [--mode fast|targeted|deep|full] [--all|--targets-only]
             [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  version
  validate-tools --keys <keys_dir> [--backend <rpf_backend_rs.exe>]

Examples:
  redux_rpf_scanner.exe compare-rpf --clean "C:\clean\update.rpf" --modded "C:\modded\update.rpf" --keys "C:\rpf_keys" --out "diff.json"

Notes:
  - This app needs the Rust backend built from rpf-rs/rpf-archive.
  - Encrypted GTA V update.rpf requires a valid keys directory.
  - Keys directory should contain:
      gtav_aes_key.dat
      gtav_ng_key.dat
      gtav_ng_decrypt_tables.dat
  - --all and --targets-only are deprecated; use --mode instead.
  - Rules files are optional; built-in rules are used if none are provided.
  - This app does not provide or extract keys.
)";
}

static bool is_valid_mode_value(const std::string& value) {
    return value == "fast" || value == "targeted" || value == "deep" || value == "full";
}

static fs::path default_backend_path(const char* argv0) {
    fs::path exe = fs::absolute(argv0);
    fs::path dir = exe.parent_path();

#ifdef _WIN32
    fs::path candidate = dir / "tools" / "rpf_backend_rs.exe";
#else
    fs::path candidate = dir / "tools" / "rpf_backend_rs";
#endif

    return candidate;
}

static Args parse_args(int argc, char** argv) {
    if (argc < 2) {
        usage();
        std::exit(1);
    }

    Args args;
    args.command = argv[1];

    if (args.command == "--help" || args.command == "-h") {
        usage();
        std::exit(0);
    }

    args.backend = default_backend_path(argv[0]);

    for (int i = 2; i < argc; ++i) {
        std::string a = argv[i];

        auto need = [&](const std::string& name) -> std::string {
            if (i + 1 >= argc) throw std::runtime_error("Missing value for " + name);
            return argv[++i];
        };

        if (a == "--clean") args.clean = need(a);
        else if (a == "--modded") args.modded = need(a);
        else if (a == "--archive") args.archive = need(a);
        else if (a == "--keys") args.keys = need(a);
        else if (a == "--out") args.out = need(a);
        else if (a == "--backend") args.backend = need(a);
        else if (a == "--depth") args.depth = std::stoi(need(a));
        else if (a == "--mode") {
            std::string value = need(a);
            if (!is_valid_mode_value(value)) {
                throw std::runtime_error("Invalid --mode value: " + value);
            }
            args.mode = value;
            args.mode_set = true;
        }
        else if (a == "--all") args.deprecated_mode = DeprecatedModeFlag::All;
        else if (a == "--targets-only") args.deprecated_mode = DeprecatedModeFlag::TargetsOnly;
        else if (a == "--component-rules") args.component_rules = need(a);
        else if (a == "--target-rules") args.target_rules = need(a);
        else if (a == "--rules-dir") args.rules_dir = need(a);
        else if (a == "--help" || a == "-h") {
            usage();
            std::exit(0);
        } else {
            throw std::runtime_error("Unknown argument: " + a);
        }
    }

    return args;
}

static void require_file(const fs::path& p, const std::string& label) {
    if (p.empty()) throw std::runtime_error("Missing required path: " + label);
    if (!fs::exists(p)) throw std::runtime_error(label + " does not exist: " + path_to_utf8(p));
    if (!fs::is_regular_file(p)) throw std::runtime_error(label + " is not a file: " + path_to_utf8(p));
}

static void require_existing_path(const fs::path& p, const std::string& label) {
    if (p.empty()) throw std::runtime_error("Missing required path: " + label);
    if (!fs::exists(p)) throw std::runtime_error(label + " does not exist: " + path_to_utf8(p));
}

static void ensure_output_parent(const fs::path& out) {
    if (out.empty()) return;
    fs::path parent = out.parent_path();
    if (parent.empty()) return;
    if (!fs::exists(parent)) {
        fs::create_directories(parent);
    }
}

static std::vector<std::string> build_backend_args(const Args& args) {
    std::vector<std::string> v;

    if (args.command == "compare-rpf") {
        require_file(args.clean, "clean update.rpf");
        require_file(args.modded, "modded update.rpf");
        if (args.out.empty()) throw std::runtime_error("Missing required --out path");

        v.push_back("compare");
        v.push_back("--clean");
        v.push_back(path_to_utf8(args.clean));
        v.push_back("--modded");
        v.push_back(path_to_utf8(args.modded));
        v.push_back("--keys");
        v.push_back(path_to_utf8(args.keys));
        v.push_back("--out");
        v.push_back(path_to_utf8(args.out));
        v.push_back("--depth");
        v.push_back(std::to_string(args.depth));
        v.push_back("--scanner-name");
        v.push_back(SCANNER_NAME);
        v.push_back("--scanner-version");
        v.push_back(SCANNER_VERSION);
    } else if (args.command == "scan-rpf") {
        require_file(args.archive, "archive");
        if (args.out.empty()) throw std::runtime_error("Missing required --out path");

        v.push_back("scan");
        v.push_back("--archive");
        v.push_back(path_to_utf8(args.archive));
        v.push_back("--keys");
        v.push_back(path_to_utf8(args.keys));
        v.push_back("--out");
        v.push_back(path_to_utf8(args.out));
        v.push_back("--depth");
        v.push_back(std::to_string(args.depth));
        v.push_back("--scanner-name");
        v.push_back(SCANNER_NAME);
        v.push_back("--scanner-version");
        v.push_back(SCANNER_VERSION);
    } else {
        usage();
        throw std::runtime_error("Unknown command: " + args.command);
    }

    if (args.mode_set) {
        v.push_back("--mode");
        v.push_back(args.mode);
    }
    if (args.deprecated_mode == DeprecatedModeFlag::All) {
        v.push_back("--all");
    } else if (args.deprecated_mode == DeprecatedModeFlag::TargetsOnly) {
        v.push_back("--targets-only");
    }

    return v;
}

#ifdef _WIN32
static int spawn_backend_windows(const fs::path& backend, const std::vector<std::string>& args) {
    const std::wstring backend_w = utf8_to_wide(path_to_utf8(backend));

    std::wstring command_line = quote_windows_arg(backend_w);

    for (const auto& arg : args) {
        command_line.push_back(L' ');
        command_line += quote_windows_arg(utf8_to_wide(arg));
    }

    std::vector<wchar_t> mutable_command_line(command_line.begin(), command_line.end());
    mutable_command_line.push_back(L'\0');

    STARTUPINFOW si{};
    si.cb = sizeof(si);

    PROCESS_INFORMATION pi{};

    BOOL ok = CreateProcessW(
        backend_w.c_str(),                 // lpApplicationName
        mutable_command_line.data(),       // lpCommandLine, mutable
        nullptr,                           // process attrs
        nullptr,                           // thread attrs
        FALSE,                             // inherit handles
        0,                                 // flags
        nullptr,                           // env
        nullptr,                           // cwd
        &si,
        &pi
    );

    if (!ok) {
        DWORD err = GetLastError();
        throw std::runtime_error("CreateProcessW failed. Windows error code: " + std::to_string(err));
    }

    WaitForSingleObject(pi.hProcess, INFINITE);

    DWORD exit_code = 1;
    GetExitCodeProcess(pi.hProcess, &exit_code);

    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);

    return static_cast<int>(exit_code);
}
#else
static int spawn_backend_posix(const fs::path& backend, const std::vector<std::string>& args) {
    std::vector<std::string> storage;
    storage.reserve(args.size() + 2);
    storage.push_back(path_to_utf8(backend));
    for (const auto& a : args) storage.push_back(a);

    std::vector<char*> argv;
    argv.reserve(storage.size() + 1);
    for (auto& s : storage) argv.push_back(s.data());
    argv.push_back(nullptr);

    pid_t pid = fork();
    if (pid == 0) {
        execv(storage[0].c_str(), argv.data());
        std::exit(127);
    }

    if (pid < 0) return 127;

    int status = 0;
    waitpid(pid, &status, 0);

    if (WIFEXITED(status)) return WEXITSTATUS(status);
    return 127;
}
#endif

static int run_backend_capture_output(const fs::path& backend, const std::vector<std::string>& args, std::string& output, std::string& error) {
#ifdef _WIN32
    const std::wstring backend_w = utf8_to_wide(path_to_utf8(backend));
    std::wstring command_line = quote_windows_arg(backend_w);
    for (const auto& arg : args) {
        command_line.push_back(L' ');
        command_line += quote_windows_arg(utf8_to_wide(arg));
    }
    std::vector<wchar_t> mutable_command_line(command_line.begin(), command_line.end());
    mutable_command_line.push_back(L'\0');

    SECURITY_ATTRIBUTES sa{};
    sa.nLength = sizeof(sa);
    sa.bInheritHandle = TRUE;
    sa.lpSecurityDescriptor = nullptr;

    HANDLE read_pipe = nullptr;
    HANDLE write_pipe = nullptr;
    if (!CreatePipe(&read_pipe, &write_pipe, &sa, 0)) {
        error = "CreatePipe failed";
        return 127;
    }
    if (!SetHandleInformation(read_pipe, HANDLE_FLAG_INHERIT, 0)) {
        CloseHandle(read_pipe);
        CloseHandle(write_pipe);
        error = "SetHandleInformation failed";
        return 127;
    }

    STARTUPINFOW si{};
    si.cb = sizeof(si);
    si.dwFlags |= STARTF_USESTDHANDLES;
    si.hStdOutput = write_pipe;
    si.hStdError = write_pipe;
    si.hStdInput = GetStdHandle(STD_INPUT_HANDLE);

    PROCESS_INFORMATION pi{};

    BOOL ok = CreateProcessW(
        backend_w.c_str(),
        mutable_command_line.data(),
        nullptr,
        nullptr,
        TRUE,
        CREATE_NO_WINDOW,
        nullptr,
        nullptr,
        &si,
        &pi
    );

    CloseHandle(write_pipe);

    if (!ok) {
        DWORD err = GetLastError();
        CloseHandle(read_pipe);
        error = "CreateProcessW failed. Windows error code: " + std::to_string(err);
        return 127;
    }

    char buffer[4096];
    DWORD bytes_read = 0;
    while (ReadFile(read_pipe, buffer, sizeof(buffer), &bytes_read, nullptr) && bytes_read > 0) {
        output.append(buffer, buffer + bytes_read);
    }

    CloseHandle(read_pipe);

    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD exit_code = 1;
    GetExitCodeProcess(pi.hProcess, &exit_code);
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return static_cast<int>(exit_code);
#else
    std::vector<std::string> storage;
    storage.reserve(args.size() + 2);
    storage.push_back(path_to_utf8(backend));
    for (const auto& a : args) storage.push_back(a);

    std::vector<char*> argv;
    argv.reserve(storage.size() + 1);
    for (auto& s : storage) argv.push_back(s.data());
    argv.push_back(nullptr);

    int pipes[2];
    if (pipe(pipes) != 0) {
        error = "pipe() failed";
        return 127;
    }

    pid_t pid = fork();
    if (pid == 0) {
        dup2(pipes[1], STDOUT_FILENO);
        dup2(pipes[1], STDERR_FILENO);
        close(pipes[0]);
        close(pipes[1]);
        execv(storage[0].c_str(), argv.data());
        std::exit(127);
    }

    close(pipes[1]);
    if (pid < 0) {
        close(pipes[0]);
        error = "fork() failed";
        return 127;
    }

    char buffer[4096];
    ssize_t read_bytes = 0;
    while ((read_bytes = read(pipes[0], buffer, sizeof(buffer))) > 0) {
        output.append(buffer, buffer + read_bytes);
    }
    close(pipes[0]);

    int status = 0;
    waitpid(pid, &status, 0);

    if (WIFEXITED(status)) return WEXITSTATUS(status);
    return 127;
#endif
}

static std::string parse_version_from_output(const std::string& output) {
    std::istringstream iss(output);
    std::string line;
    while (std::getline(iss, line)) {
        line = trim_copy(line);
        if (!line.empty()) {
            std::istringstream words(line);
            std::string token;
            std::string last;
            while (words >> token) {
                last = token;
            }
            return last;
        }
    }
    return "";
}

static int validate_tools(const Args& args) {
    std::vector<std::string> errors;

    const fs::path backend_path = args.backend;
    const bool backend_exists = !backend_path.empty() && fs::exists(backend_path) && fs::is_regular_file(backend_path);

    std::string backend_version;
    bool backend_version_ok = false;
    std::string backend_version_error;

    if (backend_exists) {
        std::string output;
        const int exit_code = run_backend_capture_output(backend_path, { "version" }, output, backend_version_error);
        if (exit_code == 0) {
            backend_version = parse_version_from_output(output);
            backend_version_ok = !backend_version.empty();
            if (!backend_version_ok) {
                backend_version_error = "Backend version output was empty.";
            }
        } else if (backend_version_error.empty()) {
            backend_version_error = "Backend version command failed with exit code " + std::to_string(exit_code) + ".";
        }
    } else {
        errors.push_back("backend not found");
    }

    const fs::path keys_path = args.keys;
    const bool keys_exists = !keys_path.empty() && fs::exists(keys_path) && fs::is_directory(keys_path);

    const fs::path aes_key = keys_path / "gtav_aes_key.dat";
    const fs::path ng_key = keys_path / "gtav_ng_key.dat";
    const fs::path ng_tables = keys_path / "gtav_ng_decrypt_tables.dat";

    const bool aes_exists = keys_exists && fs::exists(aes_key) && fs::is_regular_file(aes_key);
    const bool ng_exists = keys_exists && fs::exists(ng_key) && fs::is_regular_file(ng_key);
    const bool tables_exists = keys_exists && fs::exists(ng_tables) && fs::is_regular_file(ng_tables);

    if (keys_path.empty()) {
        errors.push_back("keys path missing");
    } else if (!keys_exists) {
        errors.push_back("keys directory not found");
    } else {
        if (!aes_exists) errors.push_back("gtav_aes_key.dat missing");
        if (!ng_exists) errors.push_back("gtav_ng_key.dat missing");
        if (!tables_exists) errors.push_back("gtav_ng_decrypt_tables.dat missing");
    }

    const bool ok = backend_exists && keys_exists && aes_exists && ng_exists && tables_exists;

    std::ostringstream json;
    json << "{\n";
    json << "  \"ok\": " << (ok ? "true" : "false") << ",\n";
    json << "  \"scanner\": {\n";
    json << "    \"name\": " << json_string(SCANNER_NAME) << ",\n";
    json << "    \"version\": " << json_string(SCANNER_VERSION) << "\n";
    json << "  },\n";
    json << "  \"backend\": {\n";
    json << "    \"path\": " << json_string(path_to_utf8(backend_path)) << ",\n";
    json << "    \"exists\": " << (backend_exists ? "true" : "false") << ",\n";
    json << "    \"version\": " << (backend_version_ok ? json_string(backend_version) : "null") << ",\n";
    json << "    \"versionOk\": " << (backend_version_ok ? "true" : "false") << ",\n";
    json << "    \"versionError\": " << (backend_version_error.empty() ? "null" : json_string(backend_version_error)) << "\n";
    json << "  },\n";
    json << "  \"keys\": {\n";
    json << "    \"path\": " << json_string(path_to_utf8(keys_path)) << ",\n";
    json << "    \"exists\": " << (keys_exists ? "true" : "false") << ",\n";
    json << "    \"files\": {\n";
    json << "      \"gtav_aes_key.dat\": " << (aes_exists ? "true" : "false") << ",\n";
    json << "      \"gtav_ng_key.dat\": " << (ng_exists ? "true" : "false") << ",\n";
    json << "      \"gtav_ng_decrypt_tables.dat\": " << (tables_exists ? "true" : "false") << "\n";
    json << "    }\n";
    json << "  },\n";
    json << "  \"errors\": [";
    for (std::size_t i = 0; i < errors.size(); ++i) {
        if (i > 0) json << ", ";
        json << json_string(errors[i]);
    }
    json << "]\n";
    json << "}\n";

    std::cout << json.str();
    return ok ? 0 : 1;
}

static int run_backend(const Args& args) {
    if (args.command != "compare-rpf" && args.command != "scan-rpf") {
        usage();
        throw std::runtime_error("Unknown command: " + args.command);
    }

    require_file(args.backend, "backend");
    require_existing_path(args.keys, "keys");
    ensure_output_parent(args.out);

    const auto backendArgs = build_backend_args(args);

    std::cout << "[backend exe] " << path_to_utf8(args.backend) << "\n";
    std::cout << "[backend args]";
    for (const auto& a : backendArgs) {
        std::cout << " [" << a << "]";
    }
    std::cout << "\n\n";

#ifdef _WIN32
    return spawn_backend_windows(args.backend, backendArgs);
#else
    return spawn_backend_posix(args.backend, backendArgs);
#endif
}

int main(int argc, char** argv) {
    try {
        Args args = parse_args(argc, argv);

        if (args.command == "version") {
            std::cout << SCANNER_NAME << " " << SCANNER_VERSION << "\n";
            return 0;
        }

        if (args.command == "validate-tools") {
            return validate_tools(args);
        }

        std::cout << "Redux RPF Component Scanner\n";
        std::cout << "C++ frontend + rpf-rs backend\n\n";

        const int code = run_backend(args);

        if (code != 0) {
            std::cerr << "\nBackend failed with exit code: " << code << "\n";
            std::cerr << "Most common causes:\n";
            std::cerr << "  1. Missing or invalid keys directory\n";
            std::cerr << "  2. Backend was not built/copied to tools/rpf_backend_rs.exe\n";
            std::cerr << "  3. The update.rpf path is wrong\n";
            return code;
        }

        std::cout << "\nDone.\n";
        if (!args.out.empty()) {
            std::cout << "Report: " << path_to_utf8(args.out) << "\n";
        }
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "ERROR: " << e.what() << "\n\n";
        usage();
        return 1;
    }
}
