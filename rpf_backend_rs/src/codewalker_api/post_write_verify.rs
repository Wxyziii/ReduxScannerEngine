use std::fs;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// Classification string the execution gate must report for a confirmed copy.
const COPIED_TEST_ARCHIVE: &str = "copied_test_archive";

/// The planned (future, never executed) restore method.
const RESTORE_METHOD: &str = "copy_backup_over_target";

// ── Tolerant views over the four input reports ──────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReplaceApplyView {
    status: Option<String>,
    replace_requests_sent: bool,
    successful_replace_count: u64,
    failed_replace_count: u64,
    original_target_sha256: Option<String>,
    post_execution_target_sha256: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct BackupView {
    target_archive_path: String,
    backup_file_path: Option<String>,
    original_hash: Option<String>,
    backup_hash: Option<String>,
    hash_verified: bool,
    safe_for_future_write: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ExecutionGateView {
    codewalker_execution_eligible: bool,
    target_archive_classification: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DryPlanView {
    planned_requests: Vec<serde_json::Value>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Normalize a path for comparison: backslashes to slashes, lowercased, trimmed.
fn normalize_for_match(raw: &str) -> String {
    raw.trim().replace('\\', "/").to_lowercase()
}

fn load<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    if !path.is_file() {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<T>(&t).ok())
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerPostWriteSafetyGate {
    CodeWalkerPostWriteSafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Compare two optional hashes case-insensitively. `None` on either side ->
/// `None` (unknown).
fn opt_hash_eq(a: &Option<String>, b: &Option<String>) -> Option<bool> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.eq_ignore_ascii_case(y)),
        _ => None,
    }
}

/// Build a read-only post-write verification + rollback-plan report.
///
/// Reads the local target file, the T0.6.5 replace-apply report, the T0.5.1
/// backup report, the T0.6.4 execution gate report, and the T0.6.3 dry replace
/// plan. Compares pre/post/backup hashes, classifies the outcome, and builds a
/// rollback PLAN pointing at the verified backup. It never restores the backup,
/// never modifies the target, never calls CodeWalker, never sends an HTTP
/// request, never uses POST, never executes an external tool, and never parses
/// RPF internals. `rollbackExecuted` and `rollbackExecutionAllowed` stay `false`.
pub fn build_codewalker_post_write_verify_report(
    target_rpf: &Path,
    replace_apply_report_path: &Path,
    backup_report_path: &Path,
    execution_gate_report_path: &Path,
    dry_replace_plan_path: &Path,
) -> Result<CodeWalkerPostWriteVerifyReport, String> {
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerPostWriteWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerPostWriteBlockedItem> = Vec::new();

    // ── Target file ──────────────────────────────────────────────────────────
    let target_rpf_exists = target_rpf.is_file();
    let target_rpf_extension_valid = target_rpf
        .extension()
        .map(|e| e.eq_ignore_ascii_case("rpf"))
        .unwrap_or(false);
    let (target_current_sha256, target_current_size_bytes) = if target_rpf_exists {
        match fs::read(target_rpf) {
            Ok(bytes) => (Some(sha256_hex(&bytes)), Some(bytes.len() as u64)),
            Err(_) => (None, None),
        }
    } else {
        (None, None)
    };
    let target_hash_computed = target_current_sha256.is_some();

    // ── Load the four reports ────────────────────────────────────────────────
    let apply_view = load::<ReplaceApplyView>(replace_apply_report_path);
    let backup_view = load::<BackupView>(backup_report_path);
    let gate_view = load::<ExecutionGateView>(execution_gate_report_path);
    let plan_view = load::<DryPlanView>(dry_replace_plan_path);

    let apply_loaded = apply_view.is_some();
    let backup_loaded = backup_view.is_some();
    let gate_loaded = gate_view.is_some();
    let plan_loaded = plan_view.is_some();

    for (name, loaded) in [
        ("replace apply report", apply_loaded),
        ("backup report", backup_loaded),
        ("execution gate report", gate_loaded),
        ("dry replace plan", plan_loaded),
    ] {
        if !loaded {
            blocked_items.push(CodeWalkerPostWriteBlockedItem {
                component: "input".to_string(),
                reason: format!("The {name} could not be read/parsed."),
                block_type: "input_report_unusable".to_string(),
            });
        }
    }

    // ── Replace apply facts ─────────────────────────────────────────────────
    let apply = apply_view.unwrap_or_default();
    let replace_apply_status = apply.status.clone();
    let replace_requests_sent = apply.replace_requests_sent;
    let successful_replace_count = apply.successful_replace_count;
    let failed_replace_count = apply.failed_replace_count;
    let pre_hash = apply.original_target_sha256.clone();
    let post_hash = apply.post_execution_target_sha256.clone();

    // ── Backup facts ─────────────────────────────────────────────────────────
    let backup = backup_view.unwrap_or_default();
    let backup_file_path = backup.backup_file_path.clone();
    let backup_file_exists = backup_file_path
        .as_ref()
        .map(|p| Path::new(p).is_file())
        .unwrap_or(false);
    let backup_hash_verified = backup.hash_verified;
    let backup_safe_for_future_write = backup.safe_for_future_write;
    let backup_target_matches_target = if backup.target_archive_path.trim().is_empty() {
        None
    } else {
        Some(
            normalize_for_match(&backup.target_archive_path)
                == normalize_for_match(&target_rpf.display().to_string()),
        )
    };

    // ── Execution gate facts ────────────────────────────────────────────────
    let gate_facts = gate_view.unwrap_or_default();
    let execution_gate_was_eligible = gate_facts.codewalker_execution_eligible;
    let copied_test_archive_confirmed =
        gate_facts.target_archive_classification == COPIED_TEST_ARCHIVE;

    // ── Dry plan facts ──────────────────────────────────────────────────────
    let plan = plan_view.unwrap_or_default();
    let dry_plan_planned_request_count = plan.planned_requests.len() as u64;

    // ── Hash comparisons ─────────────────────────────────────────────────────
    let target_hash_matches_apply_report_post_hash =
        opt_hash_eq(&target_current_sha256, &post_hash);
    let target_hash_changed_from_pre_apply = match (&target_current_sha256, &pre_hash) {
        (Some(cur), Some(pre)) => Some(!cur.eq_ignore_ascii_case(pre)),
        _ => None,
    };
    let target_hash_matches_backup_original_hash =
        opt_hash_eq(&target_current_sha256, &backup.original_hash);

    // ── Verification result ──────────────────────────────────────────────────
    let succeeded = replace_requests_sent && successful_replace_count > 0;
    let failed = replace_requests_sent && successful_replace_count == 0;
    let changed = target_hash_changed_from_pre_apply;
    let verification_result = if !replace_requests_sent {
        match changed {
            Some(false) | None => CodeWalkerPostWriteResult::NoExecutionNoChange,
            Some(true) => CodeWalkerPostWriteResult::ExecutionFailedButTargetChangedSuspicious,
        }
    } else if failed {
        match changed {
            Some(true) => CodeWalkerPostWriteResult::ExecutionFailedButTargetChangedSuspicious,
            _ => CodeWalkerPostWriteResult::ExecutionFailedNoChange,
        }
    } else if succeeded {
        match changed {
            Some(true) => CodeWalkerPostWriteResult::ExecutionSucceededTargetChanged,
            Some(false) => {
                CodeWalkerPostWriteResult::ExecutionSucceededButTargetUnchangedSuspicious
            }
            None => CodeWalkerPostWriteResult::Unknown,
        }
    } else {
        CodeWalkerPostWriteResult::Unknown
    };

    let result_is_suspicious = matches!(
        verification_result,
        CodeWalkerPostWriteResult::ExecutionFailedButTargetChangedSuspicious
            | CodeWalkerPostWriteResult::ExecutionSucceededButTargetUnchangedSuspicious
    );

    // ── Rollback availability ────────────────────────────────────────────────
    let backup_valid_for_rollback = backup_loaded
        && backup_hash_verified
        && backup_safe_for_future_write
        && backup_file_exists
        && backup_target_matches_target != Some(false);
    let rollback_available = backup_valid_for_rollback;
    let rollback_recommended = rollback_available
        && (result_is_suspicious
            || matches!(
                verification_result,
                CodeWalkerPostWriteResult::ExecutionSucceededTargetChanged
            ));

    let rollback_plan = if rollback_available {
        CodeWalkerRollbackPlan {
            rollback_plan_status: CodeWalkerRollbackPlanStatus::Ready,
            target_rpf: target_rpf.display().to_string(),
            backup_file_path: backup_file_path.clone(),
            backup_sha256: backup.backup_hash.clone(),
            target_current_sha256: target_current_sha256.clone(),
            restore_method_planned: RESTORE_METHOD.to_string(),
            rollback_requires_explicit_future_confirm: true,
            rollback_execution_supported: false,
            rollback_executed: false,
            safe_to_execute_now: false,
            reason: "A verified backup is available; restore is planned but not \
                     executed in this milestone."
                .to_string(),
        }
    } else {
        let reason = if !backup_loaded {
            "Backup report could not be read."
        } else if !backup_hash_verified {
            "Backup hash was not verified."
        } else if !backup_safe_for_future_write {
            "Backup is not marked safe for future write."
        } else if !backup_file_exists {
            "Backup file does not exist."
        } else if backup_target_matches_target == Some(false) {
            "Backup target path does not match the verification target."
        } else {
            "Backup is not usable for rollback."
        };
        warnings.push(CodeWalkerPostWriteWarning {
            code: "rollback_unavailable".to_string(),
            message: reason.to_string(),
        });
        CodeWalkerRollbackPlan {
            rollback_plan_status: CodeWalkerRollbackPlanStatus::Unavailable,
            target_rpf: target_rpf.display().to_string(),
            backup_file_path: backup_file_path.clone(),
            backup_sha256: backup.backup_hash.clone(),
            target_current_sha256: target_current_sha256.clone(),
            restore_method_planned: RESTORE_METHOD.to_string(),
            rollback_requires_explicit_future_confirm: true,
            rollback_execution_supported: false,
            rollback_executed: false,
            safe_to_execute_now: false,
            reason: reason.to_string(),
        }
    };

    // ── Status ───────────────────────────────────────────────────────────────
    let inputs_ok = target_rpf_exists
        && target_hash_computed
        && apply_loaded
        && backup_loaded
        && gate_loaded
        && plan_loaded;
    let status = if inputs_ok {
        CodeWalkerPostWriteVerifyStatus::Verified
    } else {
        CodeWalkerPostWriteVerifyStatus::InvalidInput
    };

    // ── Safety gates ─────────────────────────────────────────────────────────
    let sev = |ok: bool, blocking: bool| {
        if ok {
            CodeWalkerApiSeverity::Info
        } else if blocking {
            CodeWalkerApiSeverity::Blocking
        } else {
            CodeWalkerApiSeverity::Warning
        }
    };
    let gates = vec![
        gate(
            "target_rpf_present",
            target_rpf_exists,
            sev(target_rpf_exists, true),
            "The target archive file exists.",
        ),
        gate(
            "target_rpf_extension_valid",
            target_rpf_extension_valid,
            sev(target_rpf_extension_valid, true),
            "The target archive has a .rpf extension.",
        ),
        gate(
            "target_hash_computed",
            target_hash_computed,
            sev(target_hash_computed, true),
            "The current target SHA-256 was computed.",
        ),
        gate(
            "replace_apply_report_loaded",
            apply_loaded,
            sev(apply_loaded, true),
            "The replace apply report parsed successfully.",
        ),
        gate(
            "backup_report_loaded",
            backup_loaded,
            sev(backup_loaded, true),
            "The backup report parsed successfully.",
        ),
        gate(
            "execution_gate_report_loaded",
            gate_loaded,
            sev(gate_loaded, true),
            "The execution gate report parsed successfully.",
        ),
        gate(
            "dry_replace_plan_loaded",
            plan_loaded,
            sev(plan_loaded, true),
            "The dry replace plan parsed successfully.",
        ),
        gate(
            "copied_test_archive_confirmed",
            copied_test_archive_confirmed,
            sev(copied_test_archive_confirmed, false),
            "The execution gate classified the target as a copied test archive.",
        ),
        gate(
            "execution_gate_was_eligible",
            execution_gate_was_eligible,
            sev(execution_gate_was_eligible, false),
            "The execution gate reported eligibility.",
        ),
        gate(
            "backup_hash_verified",
            backup_hash_verified,
            sev(backup_hash_verified, false),
            "The backup was hash-verified.",
        ),
        gate(
            "backup_safe_for_future_write",
            backup_safe_for_future_write,
            sev(backup_safe_for_future_write, false),
            "The backup is marked safe for a future write.",
        ),
        gate(
            "backup_file_present",
            backup_file_exists,
            sev(backup_file_exists, false),
            "The backup file exists on disk.",
        ),
        gate(
            "backup_target_matches_target",
            backup_target_matches_target != Some(false),
            sev(backup_target_matches_target != Some(false), false),
            "The backup target path matches the verification target (or was absent).",
        ),
        gate(
            "post_write_hash_compared",
            true,
            CodeWalkerApiSeverity::Info,
            "Pre/post/backup hashes were compared.",
        ),
        gate(
            "rollback_plan_built",
            true,
            CodeWalkerApiSeverity::Info,
            "A rollback plan object was built (ready or unavailable).",
        ),
        gate(
            "rollback_execution_not_supported_yet",
            true,
            CodeWalkerApiSeverity::Info,
            "Rollback execution is not supported in this milestone.",
        ),
        gate(
            "no_http_requests_sent",
            true,
            CodeWalkerApiSeverity::Info,
            "No HTTP request of any kind was sent.",
        ),
        gate(
            "no_post_requests_sent",
            true,
            CodeWalkerApiSeverity::Info,
            "No POST request was issued.",
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
            "external_tool_not_executed",
            true,
            CodeWalkerApiSeverity::Info,
            "No external tool was executed.",
        ),
        gate(
            "native_parser_not_used",
            true,
            CodeWalkerApiSeverity::Info,
            "No native RPF parsing was performed.",
        ),
        gate(
            "native_writer_not_used",
            true,
            CodeWalkerApiSeverity::Info,
            "No native RPF writer was used.",
        ),
        gate(
            "archive_not_modified",
            true,
            CodeWalkerApiSeverity::Info,
            "The target archive was not modified by verification.",
        ),
    ];

    blocked_items.push(CodeWalkerPostWriteBlockedItem {
        component: "rollback".to_string(),
        reason: "Rollback execution is not implemented in this milestone.".to_string(),
        block_type: "rollback_execution_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerPostWriteBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerPostWriteSummary {
        total_gates: gates.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        verification_result,
        rollback_available,
        rollback_recommended,
        rollback_executed: false,
        modifies_archive: false,
    };

    Ok(CodeWalkerPostWriteVerifyReport {
        status,
        target_rpf: target_rpf.display().to_string(),
        target_rpf_exists,
        target_rpf_extension_valid,
        target_current_sha256,
        target_current_size_bytes,
        replace_apply_report_path: replace_apply_report_path.display().to_string(),
        backup_report_path: backup_report_path.display().to_string(),
        execution_gate_report_path: execution_gate_report_path.display().to_string(),
        dry_replace_plan_path: dry_replace_plan_path.display().to_string(),
        replace_apply_status,
        replace_requests_sent,
        successful_replace_count,
        failed_replace_count,
        replace_apply_original_target_sha256: pre_hash,
        replace_apply_post_execution_target_sha256: post_hash,
        target_hash_matches_apply_report_post_hash,
        target_hash_changed_from_pre_apply,
        target_hash_matches_backup_original_hash,
        backup_file_path,
        backup_file_exists,
        backup_hash_verified,
        backup_safe_for_future_write,
        backup_target_matches_target,
        execution_gate_was_eligible,
        copied_test_archive_confirmed,
        dry_plan_planned_request_count,
        verification_result,
        rollback_plan,
        rollback_available,
        rollback_recommended,
        rollback_executed: false,
        rollback_execution_allowed: false,
        http_requests_sent: false,
        post_requests_sent: false,
        replace_endpoint_called: false,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        external_tool_executed: false,
        native_parser_used: false,
        native_writer_used: false,
        modifies_archive: false,
        writer_allowed: false,
        active_adapter_name,
        null_adapter_active,
        gates,
        warnings,
        blocked_items,
        summary,
    })
}
