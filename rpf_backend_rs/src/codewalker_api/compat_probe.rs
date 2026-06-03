use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// Default CodeWalker.API base URL/port for local installs.
pub const DEFAULT_BASE_URL: &str = "http://localhost:5555";

/// Default safe (non-encrypted, ubiquitous) search filename for the probe.
pub const DEFAULT_SEARCH_FILENAME: &str = "visualsettings.dat";

const SERVICE_STATUS_PATH: &str = "/api/service-status";
const SEARCH_ENDPOINT: &str = "/api/search-file";
const REPLACE_ENDPOINT: &str = "/api/replace-file";

/// Maximum stored response-body sample length, in chars.
const MAX_BODY_SAMPLE: usize = 2048;

/// Per-probe timeout. Short — a compatibility probe must not block long.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

struct HttpResponse {
    status: u16,
    body: String,
}

/// Strip trailing slashes from a base URL (keep the scheme's `//`).
fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let no_trailing = trimmed.trim_end_matches('/');
    if no_trailing.is_empty() {
        trimmed.to_string()
    } else {
        no_trailing.to_string()
    }
}

/// A base URL is usable only if it is an `http://`/`https://` URL with a host.
fn base_url_valid(normalized: &str) -> bool {
    let rest = match normalized.strip_prefix("http://") {
        Some(r) => r,
        None => match normalized.strip_prefix("https://") {
            Some(r) => r,
            None => return false,
        },
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.split(':').next().unwrap_or("");
    !host.is_empty()
}

/// Percent-encode a query-parameter value (unreserved chars pass through).
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn parse_status_line(text: &str) -> Option<u16> {
    let first = text.lines().next()?;
    let mut parts = first.split_whitespace();
    let _http = parts.next()?;
    parts.next()?.parse::<u16>().ok()
}

/// Perform a single safe HTTP request with the given method. Only `http://` is
/// dialed (no TLS in std). `method` is restricted by the caller to GET/OPTIONS —
/// this probe never issues POST or any mutating method.
fn http_request(method: &str, url: &str) -> Result<HttpResponse, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "only http:// is supported for the compatibility probe".to_string())?;

    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rfind(':') {
        Some(i) => {
            let p = authority[i + 1..]
                .parse::<u16>()
                .map_err(|_| "invalid port in base URL".to_string())?;
            (&authority[..i], p)
        }
        None => (authority, 80u16),
    };

    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("address resolution failed: {e}"))?
        .next()
        .ok_or_else(|| "no socket address resolved".to_string())?;

    let mut stream = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT)
        .map_err(|e| format!("connect failed: {e}"))?;
    stream.set_read_timeout(Some(PROBE_TIMEOUT)).ok();
    stream.set_write_timeout(Some(PROBE_TIMEOUT)).ok();

    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: rpf_scanner-codewalker-compat-probe\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("request write failed: {e}"))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| format!("response read failed: {e}"))?;

    let text = String::from_utf8_lossy(&buf).to_string();
    let status = parse_status_line(&text).ok_or_else(|| "no HTTP status line".to_string())?;
    let body = text.splitn(2, "\r\n\r\n").nth(1).unwrap_or("").to_string();

    Ok(HttpResponse { status, body })
}

/// Length-limit a response body for safe storage in the report.
fn sample_body(body: &str) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    if body.chars().count() <= MAX_BODY_SAMPLE {
        Some(body.to_string())
    } else {
        Some(body.chars().take(MAX_BODY_SAMPLE).collect())
    }
}

/// Classify a response body into a coarse shape string + JSON-parse success.
fn classify_shape(body: &str) -> (String, bool) {
    if body.trim().is_empty() {
        return ("empty".to_string(), false);
    }
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => {
            let shape = match v {
                serde_json::Value::Array(_) => "json_array",
                serde_json::Value::Object(_) => "json_object",
                serde_json::Value::String(_) => "json_string",
                serde_json::Value::Number(_) => "json_number",
                serde_json::Value::Bool(_) => "json_bool",
                serde_json::Value::Null => "json_null",
            };
            (shape.to_string(), true)
        }
        Err(_) => ("non_json".to_string(), false),
    }
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerCompatibilitySafetyGate {
    CodeWalkerCompatibilitySafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Probe a live CodeWalker.API for compatibility with the planned replace flow,
/// using only safe non-mutating requests. See
/// [`CodeWalkerCompatibilityProbeReport`] for the full contract.
pub fn probe_codewalker_live_compatibility(
    base_url: Option<&str>,
    search_filename: Option<&str>,
    check_replace_options: bool,
) -> Result<CodeWalkerCompatibilityProbeReport, String> {
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let raw = base_url.unwrap_or(DEFAULT_BASE_URL);
    let normalized = normalize_base_url(raw);
    let valid = base_url_valid(&normalized);

    let search_filename = search_filename
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_SEARCH_FILENAME)
        .to_string();

    let probe_mode = if check_replace_options {
        CodeWalkerCompatibilityProbeMode::ExtendedNonMutating
    } else {
        CodeWalkerCompatibilityProbeMode::SafeDefault
    };

    let mut warnings: Vec<CodeWalkerCompatibilityWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerCompatibilityBlockedItem> = Vec::new();
    let mut observations: Vec<CodeWalkerCompatibilityObservation> = Vec::new();

    if !valid {
        blocked_items.push(CodeWalkerCompatibilityBlockedItem {
            component: "base_url".to_string(),
            reason: "Base URL is not a usable http(s) URL.".to_string(),
            block_type: "base_url_invalid".to_string(),
        });
    }

    // ── Helper to run one safe GET/OPTIONS and record an observation ─────────
    let root_url = format!("{normalized}/");
    let status_url = format!("{normalized}{SERVICE_STATUS_PATH}");
    let search_url = format!(
        "{normalized}{SEARCH_ENDPOINT}?filename={}",
        url_encode(&search_filename)
    );
    let replace_url = format!("{normalized}{REPLACE_ENDPOINT}");

    let mut run = |endpoint: CodeWalkerCompatibilityEndpoint,
                   method: &str,
                   url: &str|
     -> (Option<u16>, String, bool) {
        if !valid {
            observations.push(CodeWalkerCompatibilityObservation {
                endpoint,
                url: url.to_string(),
                method: method.to_string(),
                called: false,
                http_status: None,
                response_body_sample: None,
                response_json_parse_success: false,
                response_shape: "not_checked".to_string(),
                safe_to_call_again: true,
                mutating: false,
                detail: Some("base URL not valid".to_string()),
            });
            return (None, "not_checked".to_string(), false);
        }
        match http_request(method, url) {
            Ok(resp) => {
                let (shape, parsed) = classify_shape(&resp.body);
                observations.push(CodeWalkerCompatibilityObservation {
                    endpoint,
                    url: url.to_string(),
                    method: method.to_string(),
                    called: true,
                    http_status: Some(resp.status),
                    response_body_sample: sample_body(&resp.body),
                    response_json_parse_success: parsed,
                    response_shape: shape.clone(),
                    safe_to_call_again: true,
                    mutating: false,
                    detail: Some(format!("HTTP {}", resp.status)),
                });
                (Some(resp.status), shape, parsed)
            }
            Err(e) => {
                observations.push(CodeWalkerCompatibilityObservation {
                    endpoint,
                    url: url.to_string(),
                    method: method.to_string(),
                    called: true,
                    http_status: None,
                    response_body_sample: None,
                    response_json_parse_success: false,
                    response_shape: "unreachable".to_string(),
                    safe_to_call_again: true,
                    mutating: false,
                    detail: Some(e),
                });
                (None, "unreachable".to_string(), false)
            }
        }
    };

    // ── Safe GET probes ──────────────────────────────────────────────────────
    let (root_http_status, _root_shape, _r) =
        run(CodeWalkerCompatibilityEndpoint::Root, "GET", &root_url);
    let (service_status_http_status, service_status_shape, _s) = run(
        CodeWalkerCompatibilityEndpoint::ServiceStatus,
        "GET",
        &status_url,
    );
    let (search_probe_http_status, search_response_shape, _se) = run(
        CodeWalkerCompatibilityEndpoint::SearchFile,
        "GET",
        &search_url,
    );

    // ── Optional non-mutating OPTIONS on the replace endpoint ────────────────
    let mut replace_endpoint_options_http_status: Option<u16> = None;
    if check_replace_options {
        let (st, _shape, _p) = run(
            CodeWalkerCompatibilityEndpoint::ReplaceFileOptions,
            "OPTIONS",
            &replace_url,
        );
        replace_endpoint_options_http_status = st;
        if st.is_none() && valid {
            warnings.push(CodeWalkerCompatibilityWarning {
                code: "replace_options_unsupported".to_string(),
                message: "OPTIONS /api/replace-file did not return a usable status; the server \
                          may not support OPTIONS. No POST was attempted."
                    .to_string(),
            });
        }
    }

    // ── Reachability / verdicts ──────────────────────────────────────────────
    let root_reachable = root_http_status.is_some();
    let service_status_reachable = service_status_http_status.is_some();
    let search_reachable = search_probe_http_status.is_some();
    let any_reachable = root_reachable || service_status_reachable || search_reachable;

    let http_ok = |s: Option<u16>| s.map(|c| (200..400).contains(&c)).unwrap_or(false);

    // Search compatibility: reachable + 2xx/3xx + JSON array (CodeWalker returns a
    // list of matching paths). Unknown when unreachable.
    let compatible_for_search = if !search_reachable {
        None
    } else {
        Some(http_ok(search_probe_http_status) && search_response_shape == "json_array")
    };

    // Dry-replace planning needs at least a reachable status + a usable search.
    let compatible_for_dry_replace_planning = if !any_reachable {
        None
    } else {
        match compatible_for_search {
            Some(s) => Some(s && http_ok(service_status_http_status)),
            None => Some(false),
        }
    };

    // ── Gates ────────────────────────────────────────────────────────────────
    let safe_default_mode = !check_replace_options;
    let gates = vec![
        gate(
            "base_url_valid",
            valid,
            if valid {
                CodeWalkerApiSeverity::Info
            } else {
                CodeWalkerApiSeverity::Blocking
            },
            if valid {
                "Base URL is a usable http(s) URL."
            } else {
                "Base URL is not a usable http(s) URL."
            },
        ),
        gate(
            "safe_default_probe_mode",
            true,
            CodeWalkerApiSeverity::Info,
            if safe_default_mode {
                "Probe ran in safe-default mode (GET root/status/search only)."
            } else {
                "Probe ran in extended non-mutating mode (adds OPTIONS replace; no POST)."
            },
        ),
        gate(
            "root_checked_get_only",
            true,
            CodeWalkerApiSeverity::Info,
            "The root endpoint was probed with GET only.",
        ),
        gate(
            "service_status_checked_get_only",
            true,
            CodeWalkerApiSeverity::Info,
            "The /api/service-status endpoint was probed with GET only.",
        ),
        gate(
            "search_checked_get_only",
            true,
            CodeWalkerApiSeverity::Info,
            "The /api/search-file endpoint was probed with GET only.",
        ),
        gate(
            "replace_options_only_if_requested",
            true,
            CodeWalkerApiSeverity::Info,
            "The replace endpoint was only OPTIONS-probed when explicitly requested.",
        ),
        gate(
            "replace_post_not_sent",
            true,
            CodeWalkerApiSeverity::Info,
            "No POST /api/replace-file was sent.",
        ),
        gate(
            "import_endpoint_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/import was not called.",
        ),
        gate(
            "reload_services_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/reload-services was not called.",
        ),
        gate(
            "set_config_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/set-config was not called.",
        ),
        gate(
            "mutation_endpoints_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "No mutation endpoint was called.",
        ),
        gate(
            "external_tool_not_executed",
            true,
            CodeWalkerApiSeverity::Info,
            "CodeWalker was not executed as a process.",
        ),
        gate(
            "native_parser_not_used",
            true,
            CodeWalkerApiSeverity::Info,
            "No native RPF parsing was performed.",
        ),
        gate(
            "archive_not_modified",
            true,
            CodeWalkerApiSeverity::Info,
            "No RPF archive was opened or modified.",
        ),
        gate(
            "null_adapter_still_active",
            null_adapter_active,
            CodeWalkerApiSeverity::Info,
            "The active adapter remains NullRpfAdapter.",
        ),
        gate(
            "writer_allowed_false",
            true,
            CodeWalkerApiSeverity::Info,
            "Writing remains disabled (writerAllowed is false).",
        ),
    ];

    // ── Standing block ───────────────────────────────────────────────────────
    blocked_items.push(CodeWalkerCompatibilityBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerCompatibilityBlockedItem {
        component: "codewalker".to_string(),
        reason: "Live replace execution is not enabled in this milestone.".to_string(),
        block_type: "live_replace_not_enabled".to_string(),
    });

    // ── Status ───────────────────────────────────────────────────────────────
    let status = if !valid {
        CodeWalkerCompatibilityProbeStatus::InvalidBaseUrl
    } else if !any_reachable {
        CodeWalkerCompatibilityProbeStatus::Offline
    } else if compatible_for_search == Some(true) && http_ok(service_status_http_status) {
        CodeWalkerCompatibilityProbeStatus::Compatible
    } else if service_status_reachable || search_reachable {
        CodeWalkerCompatibilityProbeStatus::PartiallyCompatible
    } else {
        CodeWalkerCompatibilityProbeStatus::NotReady
    };

    let endpoints_observed = observations.len();
    let endpoints_reachable = observations
        .iter()
        .filter(|o| o.http_status.is_some())
        .count();
    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerCompatibilitySummary {
        total_gates: gates.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        endpoints_observed,
        endpoints_reachable,
        mutation_endpoints_called: false,
        modifies_archive: false,
        writer_allowed: false,
    };

    Ok(CodeWalkerCompatibilityProbeReport {
        status,
        probe_mode,
        base_url: raw.to_string(),
        normalized_base_url: normalized,
        root_checked: valid,
        root_http_status,
        service_status_checked: valid,
        service_status_http_status,
        service_status_shape,
        search_probe_checked: valid,
        search_probe_filename: search_filename,
        search_probe_http_status,
        search_response_shape,
        replace_endpoint_options_checked: check_replace_options,
        replace_endpoint_options_http_status,
        replace_endpoint_post_checked: false,
        replace_endpoint_post_sent: false,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        mutation_endpoints_called: false,
        external_tool_executed: false,
        modifies_archive: false,
        native_parser_used: false,
        compatible_for_search,
        compatible_for_dry_replace_planning,
        compatible_for_live_replace: false,
        writer_allowed: false,
        active_adapter_name,
        null_adapter_active,
        observations,
        gates,
        warnings,
        blocked_items,
        summary,
    })
}
