//! Informational external-tool detection.
//!
//! This module only *looks up* whether well-known tools appear to be available.
//! It never executes any tool that could modify files. Detection is a best-effort
//! PATH scan; a "not found" result is never an error.

use super::model::RpfProbeToolCheck;

/// Tools we report on. GUI tools (OpenIV, CodeWalker) are unlikely to be on PATH
/// and are typically reported as not found — that is fine and expected.
const KNOWN_TOOLS: &[&str] = &["OpenIV", "CodeWalker", "7z", "powershell", "cmd"];

/// On Windows, executable lookups should consider these suffixes.
#[cfg(windows)]
const EXE_SUFFIXES: &[&str] = &["", ".exe", ".bat", ".cmd", ".com"];
#[cfg(not(windows))]
const EXE_SUFFIXES: &[&str] = &[""];

/// Returns `true` if `name` (optionally with a known executable suffix) exists in
/// any PATH entry. Pure filesystem check — nothing is executed.
fn exists_on_path(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for suffix in EXE_SUFFIXES {
            let candidate = if suffix.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{}{}", name, suffix))
            };
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

/// Check a single tool by PATH lookup (case-insensitive on the base name via the
/// suffix candidates). Never executes the tool.
fn check_tool(tool: &str) -> RpfProbeToolCheck {
    let found = exists_on_path(tool);
    let detail = if found {
        format!("'{}' found on PATH (not executed).", tool)
    } else {
        format!("'{}' not found on PATH (informational only).", tool)
    };
    RpfProbeToolCheck {
        tool: tool.to_string(),
        found,
        method: "path_lookup".to_string(),
        detail,
    }
}

/// Detect the known external tools. Always returns one entry per tool; missing
/// tools are reported as `found: false`, never as errors.
pub fn detect_external_tools() -> Vec<RpfProbeToolCheck> {
    KNOWN_TOOLS.iter().map(|t| check_tool(t)).collect()
}

/// Test-only helper: confirm a clearly bogus tool name is reported not-found.
#[cfg(test)]
pub(crate) fn check_tool_for_test(name: &str) -> RpfProbeToolCheck {
    check_tool(name)
}

/// Test-only helper: PATH membership of an arbitrary name.
#[cfg(test)]
pub(crate) fn exists_on_path_for_test(name: &str) -> bool {
    exists_on_path(name)
}
