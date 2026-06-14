use std::fs;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::dry_replace::{REPLACE_ENDPOINT, SELECTED_WRITER_ROUTE};
use super::http_client::{base_url_valid, http_post_json, normalize_base_url};
use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// Default CodeWalker.API base URL used when `--base-url` is omitted.
pub const DEFAULT_BASE_URL: &str = "http://localhost:5555";

/// The exact confirmation phrase required to send any replace request.
pub const CONFIRMATION_PHRASE: &str =
    "I understand this will call CodeWalker replace on a copied test archive";

/// Classification string the execution gate must report for eligibility.
const COPIED_TEST_ARCHIVE: &str = "copied_test_archive";

/// Timeout for each replace request.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(4000);

// ── Tolerant views over the two input reports ───────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ExecutionGateView {
    codewalker_execution_eligible: bool,
    codewalker_execution_performed: bool,
    target_archive_classification: String,
    target_rpf: String,
    target_rpf_exists: bool,
    writer_allowed: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DryPlanView {
    dry_run_only: bool,
    ready_for_execution: bool,
    planned_requests: Vec<PlannedRequestView>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
struct PlannedRequestView {
    rpf_path: Option<String>,
    archive_path: Option<String>,
    source_file_path: String,
    archive_relative_path: String,
    // ── CodeWalker.API contract fields (T0.6.15) ────────────────────────────
    local_file_path: Option<String>,
    codewalker_target_path: Option<String>,
    actual_request_payload: Option<ActualPayloadView>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
struct ActualPayloadView {
    local_file_path: Option<String>,
    rpf_file_path: Option<String>,
}

impl PlannedRequestView {
    /// The absolute local replacement file path to send as `localFilePath`.
    fn local_file_path(&self) -> Option<String> {
        self.actual_request_payload
            .as_ref()
            .and_then(|p| p.local_file_path.clone())
            .or_else(|| self.local_file_path.clone())
            .filter(|s| !s.trim().is_empty())
    }

    /// The in-archive entry path to send as `rpfFilePath`.
    fn rpf_file_path(&self) -> Option<String> {
        self.actual_request_payload
            .as_ref()
            .and_then(|p| p.rpf_file_path.clone())
            .or_else(|| self.codewalker_target_path.clone())
            .or_else(|| self.rpf_path.clone())
            .filter(|s| !s.trim().is_empty())
    }

    /// True when this request carries an absolute, existing local file and a
    /// non-empty in-archive target path — required before any POST.
    fn contract_valid(&self) -> bool {
        match (self.local_file_path(), self.rpf_file_path()) {
            (Some(local), Some(_rpf)) => {
                let p = Path::new(&local);
                p.is_absolute() && p.is_file()
            }
            _ => false,
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerReplaceApplySafetyGate {
    CodeWalkerReplaceApplySafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Truncate a response body for the audit summary.
fn summarize_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.len() > 240 {
        format!("{}…", &trimmed[..240])
    } else {
        trimmed.to_string()
    }
}

/// Apply CodeWalker replaces on a COPIED TEST ARCHIVE — the first scoped executor.
///
/// Sends `POST /api/replace-file` for each planned request, but ONLY when the
/// T0.6.4 execution gate is eligible, the target is classified as a copied test
/// archive, `execute` is `true`, and `confirmation_phrase` exactly matches
/// [`CONFIRMATION_PHRASE`]. It never calls import/reload-services/set-config or
/// the search endpoint, never executes CodeWalker as a process or any external
/// tool, never parses RPF internals, and never auto-rolls-back. Global
/// `writer_allowed` stays `false`; the active adapter stays `NullRpfAdapter`. If
/// any blocking gate fails, NO HTTP request is sent and a blocked report returns.
pub fn apply_codewalker_replace_on_test_archive(
    base_url: Option<&str>,
    execution_gate_report_path: &Path,
    dry_replace_plan_path: &Path,
    execute: bool,
    confirmation_phrase: Option<&str>,
) -> Result<CodeWalkerReplaceApplyReport, String> {
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let base_url_raw = base_url.unwrap_or(DEFAULT_BASE_URL).to_string();
    let normalized_base_url = normalize_base_url(&base_url_raw);
    let base_url_ok = base_url_valid(&normalized_base_url);
    let replace_url = format!("{normalized_base_url}{REPLACE_ENDPOINT}");

    let mut warnings: Vec<CodeWalkerReplaceApplyWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerReplaceApplyBlockedItem> = Vec::new();

    // ── Authorization inputs ────────────────────────────────────────────────
    let execute_requested = execute;
    let confirmation_phrase_provided = confirmation_phrase.is_some();
    let confirmation_phrase_matched = confirmation_phrase
        .map(|p| p == CONFIRMATION_PHRASE)
        .unwrap_or(false);

    // ── Load execution gate report (T0.6.4) ─────────────────────────────────
    let (gate_view, gate_loaded): (ExecutionGateView, bool) =
        if execution_gate_report_path.is_file() {
            match fs::read_to_string(execution_gate_report_path)
                .ok()
                .and_then(|t| serde_json::from_str::<ExecutionGateView>(&t).ok())
            {
                Some(v) => (v, true),
                None => {
                    blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
                        component: "input".to_string(),
                        reason: "Execution gate report could not be parsed.".to_string(),
                        block_type: "execution_gate_report_unparsable".to_string(),
                    });
                    (ExecutionGateView::default(), false)
                }
            }
        } else {
            blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
                component: "input".to_string(),
                reason: "Execution gate report file was not found.".to_string(),
                block_type: "execution_gate_report_missing".to_string(),
            });
            (ExecutionGateView::default(), false)
        };

    let execution_gate_eligible = gate_loaded && gate_view.codewalker_execution_eligible;
    let copied_test_archive_confirmed =
        gate_loaded && gate_view.target_archive_classification == COPIED_TEST_ARCHIVE;
    let target_rpf = gate_view.target_rpf.clone();
    let target_exists = gate_loaded && gate_view.target_rpf_exists;

    // ── Load dry replace plan report (T0.6.3) ───────────────────────────────
    let (plan_view, plan_loaded): (DryPlanView, bool) = if dry_replace_plan_path.is_file() {
        match fs::read_to_string(dry_replace_plan_path)
            .ok()
            .and_then(|t| serde_json::from_str::<DryPlanView>(&t).ok())
        {
            Some(v) => (v, true),
            None => {
                blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
                    component: "input".to_string(),
                    reason: "Dry replace plan could not be parsed.".to_string(),
                    block_type: "dry_replace_plan_unparsable".to_string(),
                });
                (DryPlanView::default(), false)
            }
        }
    } else {
        blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
            component: "input".to_string(),
            reason: "Dry replace plan file was not found.".to_string(),
            block_type: "dry_replace_plan_missing".to_string(),
        });
        (DryPlanView::default(), false)
    };

    let plan_has_requests = plan_loaded && !plan_view.planned_requests.is_empty();
    let plan_dry_run_only = plan_loaded && plan_view.dry_run_only;
    // Every planned request must carry an absolute, existing localFilePath and a
    // non-empty rpfFilePath before any POST is sent (CodeWalker.API contract).
    let all_contract_payloads_valid = plan_has_requests
        && plan_view
            .planned_requests
            .iter()
            .all(|r| r.contract_valid());

    // ── Strict gates ─────────────────────────────────────────────────────────
    // All `blocking` gates must pass before any HTTP request is sent.
    struct G {
        name: &'static str,
        passed: bool,
        blocking: bool,
        message: &'static str,
    }
    let gate_specs = vec![
        G {
            name: "execution_gate_report_loaded",
            passed: gate_loaded,
            blocking: true,
            message: "The execution gate report parsed successfully.",
        },
        G {
            name: "execution_gate_eligible",
            passed: execution_gate_eligible,
            blocking: true,
            message: "The execution gate reported codewalkerExecutionEligible true.",
        },
        G {
            name: "copied_test_archive_classification_confirmed",
            passed: copied_test_archive_confirmed,
            blocking: true,
            message: "The gate classified the target as a copied test archive.",
        },
        G {
            name: "target_exists",
            passed: target_exists,
            blocking: true,
            message: "The target archive exists.",
        },
        G {
            name: "dry_replace_plan_loaded",
            passed: plan_loaded,
            blocking: true,
            message: "The dry replace plan parsed successfully.",
        },
        G {
            name: "dry_replace_plan_has_planned_requests",
            passed: plan_has_requests,
            blocking: true,
            message: "The dry replace plan has at least one planned request.",
        },
        G {
            name: "dry_replace_plan_hashes_valid",
            passed: plan_dry_run_only,
            blocking: true,
            message: "The dry replace plan was a validated dry-run plan.",
        },
        G {
            name: "local_file_paths_absolute_and_exist",
            passed: all_contract_payloads_valid,
            blocking: true,
            message: "Every planned request has an absolute, existing localFilePath \
                      and a non-empty rpfFilePath.",
        },
        G {
            name: "execute_flag_present",
            passed: execute_requested,
            blocking: true,
            message: "The explicit --execute flag was provided.",
        },
        G {
            name: "confirmation_phrase_provided",
            passed: confirmation_phrase_provided,
            blocking: true,
            message: "A confirmation phrase was provided.",
        },
        G {
            name: "confirmation_phrase_matched",
            passed: confirmation_phrase_matched,
            blocking: true,
            message: "The confirmation phrase matched exactly.",
        },
        G {
            name: "base_url_valid",
            passed: base_url_ok,
            blocking: true,
            message: "The base URL is a usable http(s) URL.",
        },
    ];

    let all_blocking_passed = gate_specs.iter().filter(|g| g.blocking).all(|g| g.passed);

    // ── Execute only when every blocking gate passes ────────────────────────
    let mut item_results: Vec<CodeWalkerReplaceApplyItemResult> = Vec::new();
    let mut successful = 0usize;
    let mut failed = 0usize;
    let mut original_sha: Option<String> = None;
    let mut post_sha: Option<String> = None;

    if all_blocking_passed {
        // Record original hash if the target is locally accessible.
        let target_path = Path::new(&target_rpf);
        original_sha = fs::read(target_path).ok().map(|b| sha256_hex(&b));
        if original_sha.is_none() {
            warnings.push(CodeWalkerReplaceApplyWarning {
                code: "target_not_locally_accessible".to_string(),
                message: "Target file is not locally accessible; hash audit will be unknown."
                    .to_string(),
            });
        }

        for req in &plan_view.planned_requests {
            // Exact CodeWalker.API `ReplaceFileForm` contract: only `localFilePath`
            // (absolute local replacement file) and `rpfFilePath` (in-archive entry
            // path). No `sourceFilePath`/`rpfPath`/`archivePath`/`execute` keys.
            let local_file_path = req.local_file_path().unwrap_or_default();
            let rpf_file_path = req.rpf_file_path().unwrap_or_default();
            let body = serde_json::json!({
                "localFilePath": local_file_path,
                "rpfFilePath": rpf_file_path,
            });
            let body_str = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());

            let request = CodeWalkerReplaceApplyRequest {
                method: "POST".to_string(),
                url: replace_url.clone(),
                endpoint: REPLACE_ENDPOINT.to_string(),
                rpf_path: Some(rpf_file_path.clone()),
                archive_path: req.archive_path.clone(),
                source_file_path: local_file_path.clone(),
                archive_relative_path: req.archive_relative_path.clone(),
                dry_run_only: false,
                request_body_json: body_str.clone(),
            };

            let response = match http_post_json(&replace_url, &body_str, REQUEST_TIMEOUT) {
                Ok(resp) => {
                    let ok = (200..300).contains(&resp.status);
                    if ok {
                        successful += 1;
                    } else {
                        failed += 1;
                    }
                    CodeWalkerReplaceApplyResponse {
                        sent: true,
                        http_status: Some(resp.status),
                        succeeded: ok,
                        response_body_summary: Some(summarize_body(&resp.body)),
                        error: None,
                    }
                }
                Err(e) => {
                    failed += 1;
                    CodeWalkerReplaceApplyResponse {
                        sent: true,
                        http_status: None,
                        succeeded: false,
                        response_body_summary: None,
                        error: Some(e),
                    }
                }
            };

            item_results.push(CodeWalkerReplaceApplyItemResult {
                archive_relative_path: req.archive_relative_path.clone(),
                codewalker_resolved_path: req.rpf_file_path(),
                source_file_path: req.local_file_path().unwrap_or_default(),
                request,
                response,
            });
        }

        // Record post-execution hash if accessible.
        post_sha = fs::read(target_path).ok().map(|b| sha256_hex(&b));
    } else {
        if plan_has_requests && !all_contract_payloads_valid {
            blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
                component: "payload".to_string(),
                reason: "A planned request lacks an absolute, existing localFilePath or a \
                         non-empty rpfFilePath; no replace request was sent."
                    .to_string(),
                block_type: "replace_payload_contract_invalid".to_string(),
            });
        }
        blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
            component: "authorization".to_string(),
            reason: "One or more blocking gates failed; no replace request was sent.".to_string(),
            block_type: "blocking_gate_failed".to_string(),
        });
    }

    let replace_request_count = item_results.len();
    let replace_requests_sent = replace_request_count > 0;
    let codewalker_execution_performed = replace_requests_sent;
    let modifies_archive = successful > 0;

    let target_hash_changed = match (&original_sha, &post_sha) {
        (Some(a), Some(b)) if a == b => CodeWalkerReplaceTargetHashChange::Unchanged,
        (Some(_), Some(_)) => CodeWalkerReplaceTargetHashChange::Changed,
        _ => CodeWalkerReplaceTargetHashChange::Unknown,
    };

    // ── Append the always-true endpoint-isolation / safety gates ────────────
    let mut gates: Vec<CodeWalkerReplaceApplySafetyGate> = gate_specs
        .iter()
        .map(|g| {
            gate(
                g.name,
                g.passed,
                if g.passed {
                    CodeWalkerApiSeverity::Info
                } else if g.blocking {
                    CodeWalkerApiSeverity::Blocking
                } else {
                    CodeWalkerApiSeverity::Warning
                },
                g.message,
            )
        })
        .collect();

    let info_gates: &[(&str, bool, &str)] = &[
        (
            "replace_endpoint_only",
            true,
            "Only /api/replace-file was ever dialed.",
        ),
        (
            "import_endpoint_not_called",
            true,
            "/api/import was not called.",
        ),
        (
            "reload_services_not_called",
            true,
            "/api/reload-services was not called.",
        ),
        (
            "set_config_not_called",
            true,
            "/api/set-config was not called.",
        ),
        (
            "search_endpoint_not_called",
            true,
            "/api/search-file was not called.",
        ),
        (
            "external_tool_not_executed",
            true,
            "No external tool was executed.",
        ),
        (
            "null_adapter_still_active",
            null_adapter_active,
            "The active adapter remains NullRpfAdapter.",
        ),
        (
            "native_parser_not_used",
            true,
            "No native RPF parsing was performed.",
        ),
        (
            "native_writer_not_used",
            true,
            "No native RPF writer was used.",
        ),
        (
            "original_hash_recorded",
            original_sha.is_some() || !all_blocking_passed,
            "Original target hash was recorded (or execution was blocked).",
        ),
        (
            "post_execution_hash_recorded_or_unknown",
            true,
            "Post-execution target hash was recorded or marked unknown.",
        ),
        (
            "no_rollback_in_this_milestone",
            true,
            "No rollback/restore was attempted in this milestone.",
        ),
        (
            "global_writer_allowed_false",
            true,
            "Global writerAllowed remains false (execution is scoped only).",
        ),
    ];
    for (name, passed, msg) in info_gates {
        gates.push(gate(name, *passed, CodeWalkerApiSeverity::Info, msg));
    }

    // ── Standing blocks ─────────────────────────────────────────────────────
    blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerReplaceApplyBlockedItem {
        component: "writer".to_string(),
        reason: "Global RPF writing remains disabled; execution is scoped to copied test archives."
            .to_string(),
        block_type: "global_writer_disabled".to_string(),
    });

    // ── Status ───────────────────────────────────────────────────────────────
    let inputs_unusable = !gate_loaded || !plan_loaded;
    let status = if !all_blocking_passed {
        if inputs_unusable {
            CodeWalkerReplaceApplyStatus::InvalidInput
        } else {
            CodeWalkerReplaceApplyStatus::Blocked
        }
    } else if failed == 0 && successful > 0 {
        CodeWalkerReplaceApplyStatus::Executed
    } else if successful > 0 {
        CodeWalkerReplaceApplyStatus::PartiallyExecuted
    } else {
        CodeWalkerReplaceApplyStatus::Failed
    };

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerReplaceApplySummary {
        total_gates: gates.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        replace_request_count,
        successful_replace_count: successful,
        failed_replace_count: failed,
        codewalker_execution_performed,
        modifies_archive,
        // Global writer stays false no matter what this scoped run did.
        writer_allowed: false,
    };

    Ok(CodeWalkerReplaceApplyReport {
        status,
        base_url: base_url_raw,
        normalized_base_url,
        execution_gate_report_path: execution_gate_report_path.display().to_string(),
        dry_replace_plan_path: dry_replace_plan_path.display().to_string(),
        target_rpf,
        execute_requested,
        confirmation_phrase_provided,
        confirmation_phrase_matched,
        expected_confirmation_phrase: CONFIRMATION_PHRASE.to_string(),
        execution_gate_eligible,
        copied_test_archive_confirmed,
        selected_writer_route: SELECTED_WRITER_ROUTE.to_string(),
        active_adapter_name,
        null_adapter_active,
        replace_endpoint: REPLACE_ENDPOINT.to_string(),
        replace_requests_sent,
        replace_request_count,
        successful_replace_count: successful,
        failed_replace_count: failed,
        codewalker_execution_performed,
        codewalker_execution_allowed_now: all_blocking_passed,
        execution_scoped_writer_allowed: all_blocking_passed,
        writer_allowed: false,
        modifies_archive,
        original_target_sha256: original_sha,
        post_execution_target_sha256: post_sha,
        target_hash_changed,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        search_endpoint_called: false,
        external_tool_executed: false,
        native_parser_used: false,
        native_writer_used: false,
        rollback_performed: false,
        gates,
        warnings,
        blocked_items,
        item_results,
        summary,
    })
}
