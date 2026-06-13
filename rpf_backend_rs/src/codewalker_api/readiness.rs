use serde_json::Value;

use super::detect::{detect_codewalker_api, SERVICE_STATUS_PATH};
use super::http_client::http_get;
use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// Look up a key in a JSON object, case-insensitively, trying several aliases.
fn find_value<'a>(obj: &'a serde_json::Map<String, Value>, aliases: &[&str]) -> Option<&'a Value> {
    for (k, v) in obj.iter() {
        let kl = k.to_ascii_lowercase();
        if aliases.iter().any(|a| a.to_ascii_lowercase() == kl) {
            return Some(v);
        }
    }
    None
}

fn as_bool_loose(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "ready" | "ok" | "online" | "running" => Some(true),
            "false" | "offline" | "stopped" | "not_ready" | "notready" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn as_string_loose(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Tolerantly parse a `/api/service-status` body. Unexpected shapes never fail —
/// fields stay `None` / `false` and `json_parse_success` records the outcome.
fn parse_service_status(body: &str) -> CodeWalkerApiServiceStatusInfo {
    let mut info = CodeWalkerApiServiceStatusInfo {
        json_parse_success: false,
        ready: None,
        status_text: None,
        services_ready: None,
        gta_path_detected: false,
        gta_path: None,
        reload_version: None,
    };

    let value: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return info,
    };
    info.json_parse_success = true;

    let obj = match value.as_object() {
        Some(o) => o,
        None => return info,
    };

    if let Some(v) = find_value(obj, &["ready", "isReady"]) {
        info.ready = as_bool_loose(v);
    }
    if let Some(v) = find_value(obj, &["status", "state"]) {
        info.status_text = as_string_loose(v);
        if info.ready.is_none() {
            info.ready = as_bool_loose(v);
        }
    }
    if let Some(v) = find_value(
        obj,
        &["servicesReady", "services_ready", "allServicesReady"],
    ) {
        info.services_ready = as_bool_loose(v);
    }
    if let Some(v) = find_value(
        obj,
        &[
            "gtaPath",
            "gta_path",
            "gtavPath",
            "gameDir",
            "gameDirectory",
        ],
    ) {
        if let Some(s) = as_string_loose(v) {
            if !s.trim().is_empty() {
                info.gta_path_detected = true;
                info.gta_path = Some(s);
            }
        }
    }
    if let Some(v) = find_value(
        obj,
        &["reloadVersion", "reload_version", "version", "apiVersion"],
    ) {
        info.reload_version = as_string_loose(v);
    }

    info
}

/// True when service status clearly reports the service is ready for search.
fn status_indicates_ready(info: &CodeWalkerApiServiceStatusInfo) -> bool {
    if info.ready == Some(true) {
        return true;
    }
    if info.services_ready == Some(true) {
        return true;
    }
    if let Some(s) = &info.status_text {
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
) -> CodeWalkerApiReadinessGate {
    CodeWalkerApiReadinessGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Probe CodeWalker.API readiness using only read-only HTTP GET requests.
///
/// Reuses [`detect_codewalker_api`], then (if reachable) does one extra safe
/// `GET /api/service-status` to capture and tolerantly parse the status body.
/// Never calls replace/import/reload-services/set-config, never issues a POST or
/// any mutation, never executes CodeWalker, never opens or modifies an RPF
/// archive. `readyForReplace`, `canWriteArchive`, and `writerAllowed` stay
/// `false`. Offline yields a valid not-ready report rather than an error.
pub fn probe_codewalker_api_readiness(
    base_url: Option<&str>,
) -> Result<CodeWalkerApiReadinessReport, String> {
    let detection = detect_codewalker_api(base_url)?;

    let normalized = detection.normalized_base_url.clone();
    let raw_base = detection.base_url.clone();
    let base_url_valid = detection.status != CodeWalkerApiDetectionStatus::InvalidBaseUrl;
    let reachable = detection.reachable;

    // Active adapter facts come from the real, safe adapter — never CodeWalker.
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerApiReadinessWarning> = Vec::new();

    // Only re-probe the read-only status endpoint when there is something to talk
    // to. Offline -> no request at all.
    let (service_status_raw, service_status_http_status, info) = if base_url_valid && reachable {
        let status_url = format!("{normalized}{SERVICE_STATUS_PATH}");
        match http_get(&status_url) {
            Ok(resp) => {
                let parsed = parse_service_status(&resp.body);
                if !parsed.json_parse_success {
                    warnings.push(CodeWalkerApiReadinessWarning {
                        code: "service_status_not_json".to_string(),
                        message: "Service status response was not valid JSON; captured raw."
                            .to_string(),
                    });
                }
                (Some(resp.body), Some(resp.status), parsed)
            }
            Err(e) => {
                warnings.push(CodeWalkerApiReadinessWarning {
                    code: "service_status_unreachable".to_string(),
                    message: format!("Service status GET failed: {e}"),
                });
                (None, None, parse_service_status(""))
            }
        }
    } else {
        (None, None, parse_service_status(""))
    };

    let ready_for_search = reachable && status_indicates_ready(&info);
    if reachable && !ready_for_search {
        warnings.push(CodeWalkerApiReadinessWarning {
            code: "readiness_unconfirmed".to_string(),
            message: "Service reachable but status did not clearly report readiness.".to_string(),
        });
    }

    let status = if !base_url_valid {
        CodeWalkerApiReadinessStatus::InvalidBaseUrl
    } else if !reachable {
        CodeWalkerApiReadinessStatus::Offline
    } else if ready_for_search {
        CodeWalkerApiReadinessStatus::Ready
    } else {
        CodeWalkerApiReadinessStatus::ReachableNotReady
    };

    let gates = vec![
        gate(
            "detection_report_built",
            true,
            CodeWalkerApiSeverity::Info,
            "Detection report was built before probing readiness.",
        ),
        gate(
            "codewalker_api_reachable",
            reachable,
            if reachable {
                CodeWalkerApiSeverity::Info
            } else {
                CodeWalkerApiSeverity::Warning
            },
            if reachable {
                "CodeWalker.API responded to a read-only probe."
            } else {
                "CodeWalker.API was not reachable."
            },
        ),
        gate(
            "service_status_endpoint_checked",
            base_url_valid,
            CodeWalkerApiSeverity::Info,
            "The /api/service-status endpoint was probed read-only.",
        ),
        gate(
            "service_status_parse_attempted",
            true,
            CodeWalkerApiSeverity::Info,
            "A tolerant JSON parse of service status was attempted.",
        ),
        gate(
            "readonly_get_only",
            true,
            CodeWalkerApiSeverity::Info,
            "Only HTTP GET requests were used.",
        ),
        gate(
            "no_post_requests_used",
            true,
            CodeWalkerApiSeverity::Info,
            "No POST request was issued.",
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
            "write_endpoints_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "No write endpoint was called.",
        ),
        gate(
            "replace_endpoint_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/replace-file was not called.",
        ),
        gate(
            "import_endpoint_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/import was not called.",
        ),
        gate(
            "mutation_endpoints_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "No mutation endpoint was called.",
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
        gate(
            "archive_not_modified",
            true,
            CodeWalkerApiSeverity::Info,
            "No RPF archive was opened or modified.",
        ),
    ];

    let blocked_items = vec![
        CodeWalkerApiReadinessBlockedItem {
            component: "writer".to_string(),
            reason: "The real RPF writer is not implemented.".to_string(),
            block_type: "real_rpf_writer_not_implemented".to_string(),
        },
        CodeWalkerApiReadinessBlockedItem {
            component: "parser".to_string(),
            reason: "Native RPF parsing is not implemented.".to_string(),
            block_type: "native_rpf_parser_not_implemented".to_string(),
        },
        CodeWalkerApiReadinessBlockedItem {
            component: "codewalker".to_string(),
            reason: "Replace/import/write endpoints are not called in this milestone.".to_string(),
            block_type: "codewalker_write_not_enabled".to_string(),
        },
        CodeWalkerApiReadinessBlockedItem {
            component: "adapter".to_string(),
            reason: "The active adapter is NullRpfAdapter and cannot write.".to_string(),
            block_type: "active_adapter_cannot_write".to_string(),
        },
    ];

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerApiReadinessSummary {
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        reachable,
        ready_for_search,
        ready_for_replace: false,
        writer_allowed: false,
    };

    Ok(CodeWalkerApiReadinessReport {
        status,
        base_url: raw_base,
        normalized_base_url: normalized,
        service_status_json_parse_success: info.json_parse_success,
        service_status_http_status,
        gta_path_detected: info.gta_path_detected,
        gta_path: info.gta_path.clone(),
        reload_version: info.reload_version.clone(),
        services_ready: info.services_ready,
        service_status_raw,
        service_status_info: info,
        codewalker_api_reachable: reachable,
        codewalker_api_ready_for_search: ready_for_search,
        codewalker_api_ready_for_replace: false,
        can_call_search_later: ready_for_search,
        can_call_replace_later: false,
        can_write_archive: false,
        write_endpoints_called: false,
        replace_endpoint_called: false,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        mutation_endpoints_called: false,
        post_requests_used: false,
        external_tool_executed: false,
        modifies_archive: false,
        writer_allowed: false,
        active_adapter_name,
        detection_report: detection,
        gates,
        warnings,
        blocked_items,
        summary,
    })
}
