use std::fs;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// The exact confirmation phrase required to perform any restore.
pub const CONFIRMATION_PHRASE: &str =
    "I understand this will restore the copied test archive from backup";

/// Always-planned (and now executed) restore method.
const RESTORE_METHOD: &str = "copy_backup_over_target";

/// Substrings (case-insensitive, forward-slash path) marking an original install.
const ORIGINAL_GAME_PATH_PATTERNS: &[&str] = &[
    "grand theft auto v",
    "gta v",
    "steamapps/common",
    "epic games/gtav",
    "rockstar games/grand theft auto v",
];

// ── Tolerant views over the two input reports ───────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PostWriteVerifyView {
    rollback_available: bool,
    rollback_executed: bool,
    copied_test_archive_confirmed: bool,
    rollback_plan: RollbackPlanView,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RollbackPlanView {
    rollback_plan_status: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct BackupView {
    target_archive_path: String,
    backup_file_path: Option<String>,
    backup_hash: Option<String>,
    hash_verified: bool,
    safe_for_future_write: bool,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn normalize_for_match(raw: &str) -> String {
    raw.trim().replace('\\', "/").to_lowercase()
}

fn looks_like_original_install(target: &Path) -> bool {
    let norm = normalize_for_match(&target.display().to_string());
    if ORIGINAL_GAME_PATH_PATTERNS.iter().any(|p| norm.contains(p)) {
        return true;
    }
    norm.ends_with("/update/update.rpf") && norm.contains("games")
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
) -> CodeWalkerRollbackRestoreSafetyGate {
    CodeWalkerRollbackRestoreSafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Execute a controlled rollback restore on a COPIED TEST archive.
///
/// Copies the verified backup file back over `target_rpf`, but ONLY when the
/// T0.6.6 post-write verification report has a ready rollback plan, the T0.5.1
/// backup report is hash-verified and safe, the recomputed backup hash matches the
/// report, the target is a copied test archive (never an original game path),
/// `execute_rollback` is `true`, and `confirmation_phrase` exactly matches
/// [`CONFIRMATION_PHRASE`]. It never calls CodeWalker, never sends an HTTP
/// request, never uses POST, never executes an external tool, never parses RPF
/// internals, and never creates a backup. Global `writer_allowed` stays `false`;
/// the active adapter stays `NullRpfAdapter`. On any blocking gate failure the
/// target is NOT modified.
pub fn execute_codewalker_rollback_restore(
    target_rpf: &Path,
    post_write_verify_report_path: &Path,
    backup_report_path: &Path,
    execute_rollback: bool,
    confirmation_phrase: Option<&str>,
) -> Result<CodeWalkerRollbackRestoreReport, String> {
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerRollbackRestoreWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerRollbackRestoreBlockedItem> = Vec::new();

    // ── Authorization ───────────────────────────────────────────────────────
    let execute_rollback_requested = execute_rollback;
    let confirmation_phrase_provided = confirmation_phrase.is_some();
    let confirmation_phrase_matched = confirmation_phrase
        .map(|p| p == CONFIRMATION_PHRASE)
        .unwrap_or(false);

    // ── Target facts ────────────────────────────────────────────────────────
    let target_rpf_exists = target_rpf.is_file();
    let target_rpf_extension_valid = target_rpf
        .extension()
        .map(|e| e.eq_ignore_ascii_case("rpf"))
        .unwrap_or(false);
    let original_install = looks_like_original_install(target_rpf);
    let target_not_original_game_archive = !original_install;
    if original_install {
        blocked_items.push(CodeWalkerRollbackRestoreBlockedItem {
            component: "target".to_string(),
            reason: "Target path looks like an original GTA V install; refusing restore."
                .to_string(),
            block_type: "original_game_archive_suspected".to_string(),
        });
    }

    // ── Load post-write verification report (T0.6.6) ────────────────────────
    let pw_view = load::<PostWriteVerifyView>(post_write_verify_report_path);
    let pw_loaded = pw_view.is_some();
    let pw = pw_view.unwrap_or_default();
    let rollback_plan_ready = pw.rollback_plan.rollback_plan_status == "ready";
    let rollback_available = pw.rollback_available;
    let copied_test_archive_confirmed = pw.copied_test_archive_confirmed;
    if pw_loaded && pw.rollback_executed {
        warnings.push(CodeWalkerRollbackRestoreWarning {
            code: "post_write_report_already_executed".to_string(),
            message: "Post-write report marks rollback already executed; proceeding is scoped \
                      to this command."
                .to_string(),
        });
    }
    if !pw_loaded {
        blocked_items.push(CodeWalkerRollbackRestoreBlockedItem {
            component: "input".to_string(),
            reason: "Post-write verification report could not be read/parsed.".to_string(),
            block_type: "post_write_verify_report_unusable".to_string(),
        });
    }

    // ── Load backup report (T0.5.1) ─────────────────────────────────────────
    let backup_view = load::<BackupView>(backup_report_path);
    let backup_loaded = backup_view.is_some();
    let backup = backup_view.unwrap_or_default();
    let backup_file_path = backup.backup_file_path.clone();
    let backup_hash_verified = backup.hash_verified;
    let backup_safe_for_future_write = backup.safe_for_future_write;
    if !backup_loaded {
        blocked_items.push(CodeWalkerRollbackRestoreBlockedItem {
            component: "input".to_string(),
            reason: "Backup report could not be read/parsed.".to_string(),
            block_type: "backup_report_unusable".to_string(),
        });
    }

    // Backup file presence + recomputed hash.
    let backup_path_opt = backup_file_path.as_ref().map(Path::new);
    let backup_file_exists = backup_path_opt.map(|p| p.is_file()).unwrap_or(false);
    let recomputed_backup_sha = if backup_file_exists {
        fs::read(backup_path_opt.unwrap())
            .ok()
            .map(|b| sha256_hex(&b))
    } else {
        None
    };
    let backup_sha256 = recomputed_backup_sha
        .clone()
        .or_else(|| backup.backup_hash.clone());
    let backup_hash_matches_report = match (&recomputed_backup_sha, &backup.backup_hash) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    };

    let backup_target_matches_target = if backup.target_archive_path.trim().is_empty() {
        None
    } else {
        Some(
            normalize_for_match(&backup.target_archive_path)
                == normalize_for_match(&target_rpf.display().to_string()),
        )
    };

    // ── Strict gates ─────────────────────────────────────────────────────────
    struct G {
        name: &'static str,
        passed: bool,
        message: &'static str,
    }
    let backup_target_ok = backup_target_matches_target != Some(false);
    let strict = vec![
        G {
            name: "target_rpf_present",
            passed: target_rpf_exists,
            message: "The target archive file exists.",
        },
        G {
            name: "target_rpf_extension_valid",
            passed: target_rpf_extension_valid,
            message: "The target archive has a .rpf extension.",
        },
        G {
            name: "target_not_original_game_archive",
            passed: target_not_original_game_archive,
            message: "The target path does not look like an original game install.",
        },
        G {
            name: "copied_test_archive_confirmed",
            passed: copied_test_archive_confirmed,
            message: "The target was confirmed as a copied test archive.",
        },
        G {
            name: "post_write_verify_report_loaded",
            passed: pw_loaded,
            message: "The post-write verification report parsed successfully.",
        },
        G {
            name: "rollback_plan_ready",
            passed: rollback_plan_ready,
            message: "The post-write report has a ready rollback plan.",
        },
        G {
            name: "rollback_available",
            passed: rollback_available,
            message: "The post-write report reports rollback available.",
        },
        G {
            name: "backup_report_loaded",
            passed: backup_loaded,
            message: "The backup report parsed successfully.",
        },
        G {
            name: "backup_file_present",
            passed: backup_file_exists,
            message: "The backup file exists on disk.",
        },
        G {
            name: "backup_hash_verified",
            passed: backup_hash_verified,
            message: "The backup report marks the backup hash-verified.",
        },
        G {
            name: "backup_hash_matches_report",
            passed: backup_hash_matches_report,
            message: "The recomputed backup SHA-256 matches the backup report.",
        },
        G {
            name: "backup_safe_for_future_write",
            passed: backup_safe_for_future_write,
            message: "The backup report marks the backup safe for a future write.",
        },
        G {
            name: "backup_target_matches_target",
            passed: backup_target_ok,
            message: "The backup target path matches the restore target (or was absent).",
        },
        G {
            name: "execute_rollback_flag_present",
            passed: execute_rollback_requested,
            message: "The explicit --execute-rollback flag was provided.",
        },
        G {
            name: "confirmation_phrase_provided",
            passed: confirmation_phrase_provided,
            message: "A confirmation phrase was provided.",
        },
        G {
            name: "confirmation_phrase_matched",
            passed: confirmation_phrase_matched,
            message: "The confirmation phrase matched exactly.",
        },
    ];

    let all_blocking_passed = strict.iter().all(|g| g.passed);

    // ── Perform the restore copy only when every gate passes ────────────────
    let mut rollback_executed = false;
    let mut target_sha256_before: Option<String> = None;
    let mut target_sha256_after: Option<String> = None;
    let mut restored_target_matches_backup: Option<bool> = None;
    let mut restore_failed = false;

    if all_blocking_passed {
        target_sha256_before = fs::read(target_rpf).ok().map(|b| sha256_hex(&b));
        let backup_path = backup_path_opt.unwrap();
        match fs::copy(backup_path, target_rpf) {
            Ok(_) => {
                rollback_executed = true;
                target_sha256_after = fs::read(target_rpf).ok().map(|b| sha256_hex(&b));
                restored_target_matches_backup =
                    match (&target_sha256_after, &recomputed_backup_sha) {
                        (Some(a), Some(b)) => Some(a.eq_ignore_ascii_case(b)),
                        _ => None,
                    };
            }
            Err(e) => {
                restore_failed = true;
                warnings.push(CodeWalkerRollbackRestoreWarning {
                    code: "restore_copy_failed".to_string(),
                    message: format!("Copying the backup over the target failed: {e}"),
                });
            }
        }
    } else {
        blocked_items.push(CodeWalkerRollbackRestoreBlockedItem {
            component: "authorization".to_string(),
            reason: "One or more blocking gates failed; the target was not modified.".to_string(),
            block_type: "blocking_gate_failed".to_string(),
        });
    }

    let modifies_archive = rollback_executed;

    // ── Gate list (strict as blocking + info gates) ─────────────────────────
    let mut gates: Vec<CodeWalkerRollbackRestoreSafetyGate> = strict
        .iter()
        .map(|g| {
            gate(
                g.name,
                g.passed,
                if g.passed {
                    CodeWalkerApiSeverity::Info
                } else {
                    CodeWalkerApiSeverity::Blocking
                },
                g.message,
            )
        })
        .collect();

    let restore_copy_ok = !all_blocking_passed || rollback_executed;
    let restored_match_ok = restored_target_matches_backup != Some(false);
    let info_gates: &[(&str, bool, &str)] = &[
        (
            "no_http_requests_sent",
            true,
            "No HTTP request of any kind was sent.",
        ),
        ("no_post_requests_sent", true, "No POST request was issued."),
        (
            "replace_endpoint_not_called",
            true,
            "/api/replace-file was not called.",
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
            "external_tool_not_executed",
            true,
            "No external tool was executed.",
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
            "null_adapter_still_active",
            null_adapter_active,
            "The active adapter remains NullRpfAdapter.",
        ),
        (
            "restore_copy_performed_if_allowed",
            restore_copy_ok,
            "The restore copy was performed when gates allowed it (or was correctly skipped).",
        ),
        (
            "restored_target_matches_backup",
            restored_match_ok,
            "The restored target matches the backup hash (or no restore occurred).",
        ),
        (
            "global_writer_allowed_false",
            true,
            "Global writerAllowed remains false (restore is scoped).",
        ),
    ];
    for (name, passed, msg) in info_gates {
        let sev = if *passed {
            CodeWalkerApiSeverity::Info
        } else {
            CodeWalkerApiSeverity::Warning
        };
        gates.push(gate(name, *passed, sev, msg));
    }

    // ── Standing block ──────────────────────────────────────────────────────
    blocked_items.push(CodeWalkerRollbackRestoreBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });

    // ── Status ───────────────────────────────────────────────────────────────
    let inputs_unusable = !pw_loaded || !backup_loaded || !target_rpf_exists;
    let status = if rollback_executed {
        CodeWalkerRollbackRestoreStatus::Restored
    } else if restore_failed {
        CodeWalkerRollbackRestoreStatus::RestoreFailed
    } else if inputs_unusable {
        CodeWalkerRollbackRestoreStatus::InvalidInput
    } else {
        CodeWalkerRollbackRestoreStatus::Blocked
    };

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerRollbackRestoreSummary {
        total_gates: gates.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        rollback_executed,
        restored_target_matches_backup,
        modifies_archive,
    };

    Ok(CodeWalkerRollbackRestoreReport {
        status,
        target_rpf: target_rpf.display().to_string(),
        backup_file_path,
        post_write_verify_report_path: post_write_verify_report_path.display().to_string(),
        backup_report_path: backup_report_path.display().to_string(),
        execute_rollback_requested,
        confirmation_phrase_provided,
        confirmation_phrase_matched,
        expected_confirmation_phrase: CONFIRMATION_PHRASE.to_string(),
        target_rpf_exists,
        target_rpf_extension_valid,
        target_classification: if copied_test_archive_confirmed {
            "copied_test_archive".to_string()
        } else if original_install {
            "original_game_archive_suspected".to_string()
        } else {
            "unknown_archive".to_string()
        },
        copied_test_archive_confirmed,
        target_not_original_game_archive,
        backup_file_exists,
        backup_hash_verified,
        backup_hash_matches_report,
        backup_safe_for_future_write,
        backup_target_matches_target,
        backup_sha256,
        rollback_plan_ready,
        rollback_available,
        rollback_execution_allowed: all_blocking_passed,
        rollback_executed,
        target_sha256_before,
        target_sha256_after,
        restored_target_matches_backup,
        restore_method: RESTORE_METHOD.to_string(),
        http_requests_sent: false,
        post_requests_sent: false,
        replace_endpoint_called: false,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        external_tool_executed: false,
        native_parser_used: false,
        native_writer_used: false,
        modifies_archive,
        writer_allowed: false,
        active_adapter_name,
        null_adapter_active,
        gates,
        warnings,
        blocked_items,
        summary,
    })
}
