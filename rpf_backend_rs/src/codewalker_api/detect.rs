use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// Default CodeWalker.API base URL/port for local installs.
pub const DEFAULT_BASE_URL: &str = "http://localhost:5555";

/// Read-only status endpoint exposed by CodeWalker.API.
pub const SERVICE_STATUS_PATH: &str = "/api/service-status";

/// Timeout for each detection probe. Short — detection must not block long.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Minimal HTTP GET response (status line + body) captured by the detector.
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) body: String,
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

/// Parse the numeric status code from an HTTP status line ("HTTP/1.1 200 OK").
fn parse_status_line(text: &str) -> Option<u16> {
    let first = text.lines().next()?;
    let mut parts = first.split_whitespace();
    let _http = parts.next()?;
    parts.next()?.parse::<u16>().ok()
}

/// Perform a single safe HTTP GET. Only `http://` is dialed (no TLS in std);
/// `https://` URLs are reported unreachable rather than handled insecurely.
pub(crate) fn http_get(url: &str) -> Result<HttpResponse, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "only http:// is supported for detection".to_string())?;

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
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: rpf_scanner-codewalker-detect\r\nAccept: */*\r\nConnection: close\r\n\r\n"
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

/// Decide whether a service-status body clearly reports readiness. Conservative:
/// only a parseable JSON object with an explicit ready signal counts.
fn parse_ready(body: &str) -> bool {
    let value: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return false,
    };
    if let Some(b) = value.get("ready").and_then(|v| v.as_bool()) {
        return b;
    }
    if let Some(s) = value.get("status").and_then(|v| v.as_str()) {
        let s = s.to_ascii_lowercase();
        return s == "ready" || s == "ok" || s == "online" || s == "running";
    }
    false
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerApiSafetyGate {
    CodeWalkerApiSafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Detect a local CodeWalker.API server using only read-only HTTP GET checks.
///
/// Checks `GET /` and `GET /api/service-status` against the (normalized) base
/// URL. Never calls replace/import/write or any mutation endpoint, never
/// executes CodeWalker as a process, and never opens or modifies an RPF archive.
/// An offline server yields a valid report with `reachable: false` rather than
/// an error. `writer_allowed` and all write capabilities stay `false`.
pub fn detect_codewalker_api(
    base_url: Option<&str>,
) -> Result<CodeWalkerApiDetectionReport, String> {
    let default_base_url_used = base_url.is_none();
    let raw = base_url.unwrap_or(DEFAULT_BASE_URL);
    let normalized = normalize_base_url(raw);
    let valid = base_url_valid(&normalized);

    // Active adapter facts come from the real, safe adapter — never CodeWalker.
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    // Read-only probes. Only ever GET / and GET /api/service-status.
    let root_url = format!("{normalized}/");
    let status_url = format!("{normalized}{SERVICE_STATUS_PATH}");

    let mut tcp_reachable = false;

    let (root_available, root_http_status, root_detail) = if valid {
        match http_get(&root_url) {
            Ok(resp) => {
                tcp_reachable = true;
                (
                    true,
                    Some(resp.status),
                    Some(format!("HTTP {}", resp.status)),
                )
            }
            Err(e) => (false, None, Some(e)),
        }
    } else {
        (false, None, Some("base URL not valid".to_string()))
    };

    let (service_status_available, service_status_http_status, status_body, status_detail) =
        if valid {
            match http_get(&status_url) {
                Ok(resp) => {
                    tcp_reachable = true;
                    let detail = format!("HTTP {}", resp.status);
                    (true, Some(resp.status), resp.body, Some(detail))
                }
                Err(e) => (false, None, String::new(), Some(e)),
            }
        } else {
            (
                false,
                None,
                String::new(),
                Some("base URL not valid".to_string()),
            )
        };

    let codewalker_api_detected = service_status_available;
    let codewalker_ready = service_status_available && parse_ready(&status_body);
    let reachable = root_available || service_status_available;

    let endpoint_checks = vec![
        CodeWalkerApiEndpointCheck {
            name: "root".to_string(),
            url: root_url,
            method: "GET".to_string(),
            checked: valid,
            available: root_available,
            http_status: root_http_status,
            detail: root_detail,
        },
        CodeWalkerApiEndpointCheck {
            name: "service_status".to_string(),
            url: status_url,
            method: "GET".to_string(),
            checked: valid,
            available: service_status_available,
            http_status: service_status_http_status,
            detail: status_detail,
        },
    ];

    let capabilities = vec![
        CodeWalkerApiCapability {
            name: "detect".to_string(),
            available: true,
            description: "Read-only HTTP detection of a local CodeWalker.API.".to_string(),
        },
        CodeWalkerApiCapability {
            name: "readonly_status".to_string(),
            available: true,
            description: "Read-only GET of root and /api/service-status.".to_string(),
        },
        CodeWalkerApiCapability {
            name: "replace_file".to_string(),
            available: false,
            description: "Calling the replace endpoint is not implemented in this milestone."
                .to_string(),
        },
        CodeWalkerApiCapability {
            name: "import_file".to_string(),
            available: false,
            description: "Calling the import endpoint is not implemented in this milestone."
                .to_string(),
        },
        CodeWalkerApiCapability {
            name: "write_archive".to_string(),
            available: false,
            description: "Writing an RPF archive is not implemented.".to_string(),
        },
    ];

    let blocked_items = vec![
        CodeWalkerApiBlockedItem {
            component: "writer".to_string(),
            reason: "The real RPF writer is not implemented.".to_string(),
            block_type: "real_rpf_writer_not_implemented".to_string(),
        },
        CodeWalkerApiBlockedItem {
            component: "parser".to_string(),
            reason: "Native RPF parsing is not implemented.".to_string(),
            block_type: "native_rpf_parser_not_implemented".to_string(),
        },
        CodeWalkerApiBlockedItem {
            component: "codewalker".to_string(),
            reason: "CodeWalker write/replace/import endpoints are not called in this milestone."
                .to_string(),
            block_type: "codewalker_write_not_enabled".to_string(),
        },
        CodeWalkerApiBlockedItem {
            component: "adapter".to_string(),
            reason: "The active adapter is NullRpfAdapter and cannot write.".to_string(),
            block_type: "active_adapter_cannot_write".to_string(),
        },
    ];

    let safety_gates = vec![
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
            "readonly_detection_only",
            true,
            CodeWalkerApiSeverity::Info,
            "Only read-only GET checks were performed.",
        ),
        gate(
            "root_endpoint_checked",
            valid,
            CodeWalkerApiSeverity::Info,
            "The root endpoint was probed read-only.",
        ),
        gate(
            "service_status_endpoint_checked",
            valid,
            CodeWalkerApiSeverity::Info,
            "The /api/service-status endpoint was probed read-only.",
        ),
        gate(
            "write_endpoints_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "No write/mutation endpoint was called.",
        ),
        gate(
            "replace_endpoint_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "The replace endpoint was not called.",
        ),
        gate(
            "external_tool_not_executed",
            true,
            CodeWalkerApiSeverity::Info,
            "CodeWalker was not executed as a process.",
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

    let endpoints_available = endpoint_checks.iter().filter(|c| c.available).count();
    let passed_gate_count = safety_gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = safety_gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerApiSummary {
        endpoints_checked: endpoint_checks.len(),
        endpoints_available,
        blocked_count: blocked_items.len(),
        passed_gate_count,
        blocking_gate_count,
        reachable,
        codewalker_api_detected,
        writer_allowed: false,
    };

    let status = if !valid {
        CodeWalkerApiDetectionStatus::InvalidBaseUrl
    } else if codewalker_api_detected {
        CodeWalkerApiDetectionStatus::Detected
    } else if reachable {
        CodeWalkerApiDetectionStatus::Reachable
    } else {
        CodeWalkerApiDetectionStatus::Offline
    };

    Ok(CodeWalkerApiDetectionReport {
        status,
        base_url: raw.to_string(),
        normalized_base_url: normalized,
        default_base_url_used,
        reachable,
        tcp_reachable,
        service_status_checked: valid,
        service_status_available,
        service_status_http_status,
        root_checked: valid,
        root_available,
        root_http_status,
        codewalker_api_detected,
        codewalker_ready,
        can_detect: true,
        can_call_readonly_status: true,
        can_replace_file: false,
        can_import_file: false,
        can_write_archive: false,
        write_endpoints_checked: false,
        write_endpoints_called: false,
        replace_endpoint_called: false,
        import_endpoint_called: false,
        external_tool_executed: false,
        modifies_archive: false,
        writer_allowed: false,
        active_adapter_name,
        endpoint_checks,
        capabilities,
        blocked_items,
        safety_gates,
        summary,
    })
}
