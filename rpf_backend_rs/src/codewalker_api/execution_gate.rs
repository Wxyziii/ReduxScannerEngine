use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// Substrings (matched case-insensitively on a forward-slash path) that mark an
/// obvious original GTA V install. Conservative blocking is intended.
const ORIGINAL_GAME_PATH_PATTERNS: &[&str] = &[
    "grand theft auto v",
    "gta v",
    "steamapps/common",
    "epic games/gtav",
    "rockstar games/grand theft auto v",
];

// ── Tolerant views over existing JSON reports ───────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DryReplacePlanView {
    planned_requests: Vec<serde_json::Value>,
    dry_run_only: bool,
    ready_for_execution: bool,
    writer_allowed: bool,
    replace_endpoint_called: bool,
    post_requests_sent: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PermissionReportView {
    permission_token: Option<serde_json::Value>,
    confirmation_phrase_matched: bool,
    writer_allowed: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReadinessReportView {
    ready_to_write: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EntryManifestReportView {
    manifest: EntryManifestInnerView,
    ready_for_write: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EntryManifestInnerView {
    entries: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct BackupReportView {
    target_archive_path: Option<String>,
    hash_verified: bool,
    safe_for_future_write: bool,
}

/// Load + parse one JSON report. Returns `Some(view)` only on a clean parse.
fn load_report<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> (CodeWalkerExecutionInputReportStatus, Option<T>) {
    if !path.is_file() {
        return (CodeWalkerExecutionInputReportStatus::Missing, None);
    }
    match fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<T>(&t).ok())
    {
        Some(v) => (CodeWalkerExecutionInputReportStatus::Valid, Some(v)),
        None => (CodeWalkerExecutionInputReportStatus::Unparsable, None),
    }
}

/// Normalize a path for pattern matching: backslashes to slashes, lowercased.
fn normalize_for_match(raw: &str) -> String {
    raw.trim().replace('\\', "/").to_lowercase()
}

/// True when the path looks like an original GTA V install.
fn looks_like_original_install(target: &Path) -> bool {
    let norm = normalize_for_match(&target.display().to_string());
    if ORIGINAL_GAME_PATH_PATTERNS.iter().any(|p| norm.contains(p)) {
        return true;
    }
    // /update/update.rpf under a game-like path.
    norm.ends_with("/update/update.rpf") && norm.contains("games")
}

fn classify_target(
    target: &Path,
    exists: bool,
    extension_valid: bool,
    target_is_test_copy: bool,
) -> CodeWalkerTargetArchiveClassification {
    if !exists {
        return CodeWalkerTargetArchiveClassification::Missing;
    }
    if !extension_valid {
        return CodeWalkerTargetArchiveClassification::InvalidExtension;
    }
    if looks_like_original_install(target) {
        return CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected;
    }
    if target_is_test_copy {
        CodeWalkerTargetArchiveClassification::CopiedTestArchive
    } else {
        CodeWalkerTargetArchiveClassification::UnknownArchive
    }
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerExecutionGate {
    CodeWalkerExecutionGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Build a read-only CodeWalker copied-test-archive execution gate report.
///
/// Decides whether a FUTURE CodeWalker replace attempt against `target_rpf` would
/// even be eligible. Reads only the local target fixture and the five local
/// report files. Issues NO HTTP request, never uses POST, never calls replace/
/// import/reload-services/set-config or any mutation endpoint, never executes
/// CodeWalker or any external tool, and never opens or modifies an RPF archive.
/// Even when `codewalker_execution_eligible` is `true`, nothing is executed:
/// `codewalker_execution_allowed_now`, `codewalker_execution_performed`,
/// `writer_allowed`, and `modifies_archive` all stay `false`.
#[allow(clippy::too_many_arguments)]
pub fn build_codewalker_execution_gate_report(
    target_rpf: &Path,
    dry_replace_plan_path: &Path,
    permission_report_path: &Path,
    readiness_report_path: &Path,
    entry_manifest_report_path: &Path,
    backup_report_path: &Path,
    target_is_test_copy: bool,
) -> Result<CodeWalkerExecutionGateReport, String> {
    // Active adapter facts come from the real, safe adapter — never CodeWalker.
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerExecutionGateWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerExecutionGateBlockedItem> = Vec::new();

    // ── Target archive ──────────────────────────────────────────────────────
    let target_rpf_exists = target_rpf.is_file();
    let target_rpf_extension_valid = target_rpf
        .extension()
        .map(|e| e.eq_ignore_ascii_case("rpf"))
        .unwrap_or(false);
    let classification = classify_target(
        target_rpf,
        target_rpf_exists,
        target_rpf_extension_valid,
        target_is_test_copy,
    );
    let target_path_allowed_for_test_execution =
        classification == CodeWalkerTargetArchiveClassification::CopiedTestArchive;
    let target_not_original =
        classification != CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected;

    if classification == CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected {
        blocked_items.push(CodeWalkerExecutionGateBlockedItem {
            component: "target".to_string(),
            reason: "Target path looks like an original GTA V install; refusing eligibility."
                .to_string(),
            block_type: "original_game_archive_suspected".to_string(),
        });
    }
    if classification == CodeWalkerTargetArchiveClassification::UnknownArchive {
        blocked_items.push(CodeWalkerExecutionGateBlockedItem {
            component: "target".to_string(),
            reason: "Target was not confirmed as a copied test archive.".to_string(),
            block_type: "target_not_confirmed_test_copy".to_string(),
        });
    }

    // ── Load the five input reports (tolerant) ──────────────────────────────
    let (dry_status, dry_view) = load_report::<DryReplacePlanView>(dry_replace_plan_path);
    let (perm_status, perm_view) = load_report::<PermissionReportView>(permission_report_path);
    let (ready_status, _ready_view) = load_report::<ReadinessReportView>(readiness_report_path);
    let (manifest_status, manifest_view) =
        load_report::<EntryManifestReportView>(entry_manifest_report_path);
    let (backup_status, backup_view) = load_report::<BackupReportView>(backup_report_path);

    // ── Extract facts from the loaded reports ───────────────────────────────
    let dry_plan_loaded = dry_status == CodeWalkerExecutionInputReportStatus::Valid;
    let dry_plan_has_planned_requests = dry_view
        .as_ref()
        .map(|v| !v.planned_requests.is_empty())
        .unwrap_or(false);
    let dry_plan_dry_run_only = dry_view.as_ref().map(|v| v.dry_run_only).unwrap_or(false);
    let dry_plan_ready_for_execution = dry_view
        .as_ref()
        .map(|v| v.ready_for_execution)
        .unwrap_or(false);
    let dry_plan_no_http = dry_view
        .as_ref()
        .map(|v| !v.replace_endpoint_called && !v.post_requests_sent)
        .unwrap_or(false);

    let permission_loaded = perm_status == CodeWalkerExecutionInputReportStatus::Valid;
    let permission_token_present = perm_view
        .as_ref()
        .map(|v| v.permission_token.is_some())
        .unwrap_or(false);
    let permission_confirmation_matched = perm_view
        .as_ref()
        .map(|v| v.confirmation_phrase_matched)
        .unwrap_or(false);

    let readiness_loaded = ready_status == CodeWalkerExecutionInputReportStatus::Valid;

    let manifest_loaded = manifest_status == CodeWalkerExecutionInputReportStatus::Valid;
    let entry_manifest_has_entries = manifest_view
        .as_ref()
        .map(|v| !v.manifest.entries.is_empty())
        .unwrap_or(false);

    let backup_loaded = backup_status == CodeWalkerExecutionInputReportStatus::Valid;
    let backup_hash_verified = backup_view
        .as_ref()
        .map(|v| v.hash_verified)
        .unwrap_or(false);
    let backup_safe_for_future_write = backup_view
        .as_ref()
        .map(|v| v.safe_for_future_write)
        .unwrap_or(false);
    // Target match: pass if the backup omits the field, else compare normalized.
    let backup_target_matches = match backup_view
        .as_ref()
        .and_then(|v| v.target_archive_path.as_ref())
    {
        Some(p) => normalize_for_match(p) == normalize_for_match(&target_rpf.display().to_string()),
        None => true,
    };

    // ── Per-report validity (did the report satisfy its expectations?) ──────
    let dry_replace_plan_valid = dry_plan_loaded
        && dry_plan_has_planned_requests
        && dry_plan_dry_run_only
        && dry_plan_no_http;
    let permission_report_valid =
        permission_loaded && permission_token_present && permission_confirmation_matched;
    let readiness_report_valid = readiness_loaded;
    let entry_manifest_report_valid = manifest_loaded && entry_manifest_has_entries;
    let backup_report_valid = backup_loaded
        && backup_hash_verified
        && backup_safe_for_future_write
        && backup_target_matches;

    // Per-report status: downgrade a parsed-but-unsatisfied report to Invalid.
    let downgrade = |status: CodeWalkerExecutionInputReportStatus, valid: bool| {
        if status == CodeWalkerExecutionInputReportStatus::Valid && !valid {
            CodeWalkerExecutionInputReportStatus::Invalid
        } else {
            status
        }
    };
    let dry_replace_plan_status = downgrade(dry_status, dry_replace_plan_valid);
    let permission_report_status = downgrade(perm_status, permission_report_valid);
    let readiness_report_status = downgrade(ready_status, readiness_report_valid);
    let entry_manifest_report_status = downgrade(manifest_status, entry_manifest_report_valid);
    let backup_report_status = downgrade(backup_status, backup_report_valid);

    for (name, status) in [
        ("dry replace plan", dry_status),
        ("permission report", perm_status),
        ("readiness report", ready_status),
        ("entry manifest report", manifest_status),
        ("backup report", backup_status),
    ] {
        match status {
            CodeWalkerExecutionInputReportStatus::Missing => {
                blocked_items.push(CodeWalkerExecutionGateBlockedItem {
                    component: "input".to_string(),
                    reason: format!("The {name} file was not found."),
                    block_type: "input_report_missing".to_string(),
                });
            }
            CodeWalkerExecutionInputReportStatus::Unparsable => {
                warnings.push(CodeWalkerExecutionGateWarning {
                    code: "input_report_unparsable".to_string(),
                    message: format!("The {name} could not be parsed."),
                });
            }
            _ => {}
        }
    }

    // ── Strict gates (all must pass for eligibility) ────────────────────────
    let strict: Vec<(&str, bool, &str)> = vec![
        (
            "target_rpf_present",
            target_rpf_exists,
            "The target archive file exists.",
        ),
        (
            "target_rpf_extension_valid",
            target_rpf_extension_valid,
            "The target archive has a .rpf extension.",
        ),
        (
            "target_marked_as_test_copy",
            target_is_test_copy,
            "The target was explicitly marked/confirmed as a test copy.",
        ),
        (
            "target_not_original_game_archive",
            target_not_original,
            "The target path does not look like an original game install.",
        ),
        (
            "target_path_allowed_for_test_execution",
            target_path_allowed_for_test_execution,
            "The target classified as a copied test archive.",
        ),
        (
            "dry_replace_plan_loaded",
            dry_plan_loaded,
            "The dry replace plan parsed successfully.",
        ),
        (
            "dry_replace_plan_has_planned_requests",
            dry_plan_has_planned_requests,
            "The dry replace plan has at least one planned request.",
        ),
        (
            "dry_replace_plan_was_dry_run_only",
            dry_plan_dry_run_only,
            "The dry replace plan was dry-run only.",
        ),
        (
            "dry_replace_plan_sent_no_http_requests",
            dry_plan_no_http,
            "The dry replace plan recorded no HTTP/replace requests.",
        ),
        (
            "permission_report_loaded",
            permission_loaded,
            "The permission report parsed successfully.",
        ),
        (
            "permission_token_present",
            permission_token_present,
            "A permission token is present.",
        ),
        (
            "permission_confirmation_matched",
            permission_confirmation_matched,
            "The permission confirmation phrase matched.",
        ),
        (
            "readiness_report_loaded",
            readiness_loaded,
            "The write readiness report parsed successfully.",
        ),
        (
            "entry_manifest_report_loaded",
            manifest_loaded,
            "The entry manifest report parsed successfully.",
        ),
        (
            "entry_manifest_has_entries",
            entry_manifest_has_entries,
            "The entry manifest has at least one entry.",
        ),
        (
            "backup_report_loaded",
            backup_loaded,
            "The backup report parsed successfully.",
        ),
        (
            "backup_hash_verified",
            backup_hash_verified,
            "The backup was hash-verified.",
        ),
        (
            "backup_safe_for_future_write",
            backup_safe_for_future_write,
            "The backup report marks the target safe for a future write.",
        ),
        (
            "backup_target_matches_execution_target",
            backup_target_matches,
            "The backup target path matches the execution target (or was absent).",
        ),
    ];

    let strict_gates_all_passed = strict.iter().all(|(_, p, _)| *p);

    let mut gates: Vec<CodeWalkerExecutionGate> = strict
        .iter()
        .map(|(n, p, m)| {
            gate(
                n,
                *p,
                if *p {
                    CodeWalkerApiSeverity::Info
                } else {
                    CodeWalkerApiSeverity::Blocking
                },
                m,
            )
        })
        .collect();

    // ── Always-true safety info gates (never gate eligibility) ──────────────
    let info_gates: &[(&str, &str)] = &[
        (
            "null_adapter_still_active",
            "The active adapter remains NullRpfAdapter.",
        ),
        (
            "no_http_requests_sent",
            "No HTTP request of any kind was sent.",
        ),
        ("no_post_requests_sent", "No POST request was issued."),
        (
            "replace_endpoint_not_called",
            "/api/replace-file was not called.",
        ),
        ("import_endpoint_not_called", "/api/import was not called."),
        (
            "reload_services_not_called",
            "/api/reload-services was not called.",
        ),
        ("set_config_not_called", "/api/set-config was not called."),
        (
            "external_tool_not_executed",
            "No external tool was executed.",
        ),
        ("archive_not_modified", "No RPF archive was modified."),
        (
            "writer_allowed_false",
            "Writing remains disabled (writerAllowed is false).",
        ),
        (
            "execution_not_performed",
            "No CodeWalker execution was performed in this milestone.",
        ),
    ];
    for (name, msg) in info_gates {
        let passed = if *name == "null_adapter_still_active" {
            null_adapter_active
        } else {
            true
        };
        gates.push(gate(name, passed, CodeWalkerApiSeverity::Info, msg));
    }

    // ── Status / verdict ─────────────────────────────────────────────────────
    let any_input_unusable = matches!(
        dry_status,
        CodeWalkerExecutionInputReportStatus::Missing
            | CodeWalkerExecutionInputReportStatus::Unparsable
    ) || matches!(
        perm_status,
        CodeWalkerExecutionInputReportStatus::Missing
            | CodeWalkerExecutionInputReportStatus::Unparsable
    ) || matches!(
        ready_status,
        CodeWalkerExecutionInputReportStatus::Missing
            | CodeWalkerExecutionInputReportStatus::Unparsable
    ) || matches!(
        manifest_status,
        CodeWalkerExecutionInputReportStatus::Missing
            | CodeWalkerExecutionInputReportStatus::Unparsable
    ) || matches!(
        backup_status,
        CodeWalkerExecutionInputReportStatus::Missing
            | CodeWalkerExecutionInputReportStatus::Unparsable
    ) || !target_rpf_exists;

    // Eligibility requires every strict gate to pass. Even so, NOTHING runs.
    let codewalker_execution_eligible = strict_gates_all_passed;

    let status = if any_input_unusable {
        CodeWalkerExecutionGateStatus::InvalidInput
    } else if codewalker_execution_eligible {
        CodeWalkerExecutionGateStatus::Eligible
    } else {
        CodeWalkerExecutionGateStatus::Blocked
    };

    // Standing blocks that remain regardless of eligibility.
    blocked_items.push(CodeWalkerExecutionGateBlockedItem {
        component: "writer".to_string(),
        reason: "The real RPF writer is not implemented.".to_string(),
        block_type: "real_rpf_writer_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerExecutionGateBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerExecutionGateBlockedItem {
        component: "codewalker".to_string(),
        reason: "CodeWalker execution is not implemented and not enabled in this milestone."
            .to_string(),
        block_type: "codewalker_execution_not_enabled".to_string(),
    });

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerExecutionGateSummary {
        total_gates: gates.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        strict_gates_all_passed,
        codewalker_execution_eligible,
        // Always false this milestone, no matter how clean the gate is.
        codewalker_execution_allowed_now: false,
        codewalker_execution_performed: false,
        writer_allowed: false,
        modifies_archive: false,
    };

    Ok(CodeWalkerExecutionGateReport {
        status,
        target_rpf: target_rpf.display().to_string(),
        target_rpf_exists,
        target_rpf_extension_valid,
        target_archive_classification: classification,
        target_marked_as_test_copy: target_is_test_copy,
        target_path_allowed_for_test_execution,
        dry_replace_plan_path: dry_replace_plan_path.display().to_string(),
        permission_report_path: permission_report_path.display().to_string(),
        readiness_report_path: readiness_report_path.display().to_string(),
        entry_manifest_report_path: entry_manifest_report_path.display().to_string(),
        backup_report_path: backup_report_path.display().to_string(),
        dry_replace_plan_status,
        permission_report_status,
        readiness_report_status,
        entry_manifest_report_status,
        backup_report_status,
        dry_replace_plan_valid,
        permission_report_valid,
        readiness_report_valid,
        entry_manifest_report_valid,
        backup_report_valid,
        backup_hash_verified,
        permission_token_present,
        dry_plan_has_planned_requests,
        dry_plan_ready_for_execution,
        codewalker_execution_eligible,
        codewalker_execution_performed: false,
        codewalker_execution_allowed_now: false,
        writer_allowed: false,
        active_adapter_name,
        null_adapter_active,
        replace_endpoint_called: false,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        post_requests_sent: false,
        http_requests_sent: false,
        external_tool_executed: false,
        modifies_archive: false,
        real_writer_implemented: false,
        native_parser_implemented: false,
        gates,
        warnings,
        blocked_items,
        summary,
    })
}
