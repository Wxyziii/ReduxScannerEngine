use std::fs;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::model::*;
use super::post_write_verify::build_codewalker_post_write_verify_report;
use super::replace_apply::{
    apply_codewalker_replace_on_test_archive, CONFIRMATION_PHRASE as REPLACE_APPLY_PHRASE,
};

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// The exact confirmation phrase required before execute mode runs anything.
pub const CONFIRMATION_PHRASE: &str =
    "I understand this will run the real copied archive CodeWalker test";

/// Default planned CodeWalker.API base URL.
const DEFAULT_BASE_URL: &str = "http://localhost:5555";

/// Classification string the execution gate must report for a copied test archive.
const COPIED_TEST_ARCHIVE: &str = "copied_test_archive";

/// Filenames the coordinator writes under the project dir in execute mode.
const REPLACE_APPLY_REPORT_NAME: &str = "replace_apply_report.json";
const POST_WRITE_VERIFY_REPORT_NAME: &str = "post_write_verify_report.json";

/// Substrings (case-insensitive, forward-slash path) marking an original install.
const ORIGINAL_GAME_PATH_PATTERNS: &[&str] = &[
    "grand theft auto v",
    "gta v",
    "steamapps/common",
    "epic games/gtav",
    "rockstar games/grand theft auto v",
];

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

// ── Tolerant views over the input reports ───────────────────────────────────

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

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CompatProbeView {
    /// `Some(true)`/`Some(false)`/`None` — unknown is not blocking.
    compatible_for_search: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReplaceApplyOutcomeView {
    replace_requests_sent: bool,
    successful_replace_count: u64,
    modifies_archive: bool,
}

/// Read + JSON-parse a report into a tolerant view. Returns `(view, loaded)`.
fn load_view<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> (T, bool) {
    if !path.is_file() {
        return (T::default(), false);
    }
    match fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<T>(&t).ok())
    {
        Some(v) => (v, true),
        None => (T::default(), false),
    }
}

fn json_loadable(path: &Path) -> bool {
    path.is_file()
        && fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .is_some()
}

fn classify_target(
    target: &Path,
    exists: bool,
    extension_valid: bool,
    is_test_copy: bool,
) -> CodeWalkerTargetArchiveClassification {
    if looks_like_original_install(target) {
        return CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected;
    }
    if !exists {
        return CodeWalkerTargetArchiveClassification::Missing;
    }
    if !extension_valid {
        return CodeWalkerTargetArchiveClassification::InvalidExtension;
    }
    if is_test_copy {
        CodeWalkerTargetArchiveClassification::CopiedTestArchive
    } else {
        CodeWalkerTargetArchiveClassification::UnknownArchive
    }
}

fn base_url_is_valid(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("http://") && u.len() > "http://".len())
        || (u.starts_with("https://") && u.len() > "https://".len())
}

fn step(
    index: usize,
    command_name: &str,
    title: &str,
    description: &str,
    mutates_archive: bool,
) -> CodeWalkerTestRunStep {
    CodeWalkerTestRunStep {
        index,
        command_name: command_name.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        mutates_archive,
        executed: false,
        skipped: false,
        note: None,
    }
}

/// Coordinate (and optionally run) a full copied-archive CodeWalker replace cycle.
///
/// See [`CodeWalkerTestRunReport`] for the full contract. Plan mode (default) only
/// validates inputs and builds a planned step sequence; it sends NO HTTP request,
/// calls nothing, and never modifies the target. Execute mode requires the exact
/// [`CONFIRMATION_PHRASE`] and every eligibility gate to pass; only then does it
/// invoke the existing replace apply (copied test archives only) followed by
/// post-write verification. It never rolls back automatically, never targets an
/// original game archive, never executes CodeWalker as a process, and never parses
/// RPF internals. Global `writer_allowed` stays `false`; the adapter stays
/// `NullRpfAdapter`.
#[allow(clippy::too_many_arguments)]
pub fn build_or_run_codewalker_copied_archive_test(
    target_rpf: &Path,
    base_url: Option<&str>,
    project_dir: &Path,
    backup_report_path: &Path,
    readiness_report_path: &Path,
    entry_manifest_report_path: &Path,
    resolve_report_path: &Path,
    dry_replace_plan_path: &Path,
    execution_gate_report_path: &Path,
    compatibility_probe_report_path: Option<&Path>,
    execute: bool,
    confirmation_phrase: Option<&str>,
) -> Result<CodeWalkerTestRunReport, String> {
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerTestRunWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerTestRunBlockedItem> = Vec::new();

    let mode = if execute {
        CodeWalkerTestRunMode::ExecuteReplace
    } else {
        CodeWalkerTestRunMode::PlanOnly
    };

    // ── Target facts ─────────────────────────────────────────────────────────
    let target_rpf_exists = target_rpf.is_file();
    let target_rpf_extension_valid = target_rpf
        .extension()
        .map(|e| e.eq_ignore_ascii_case("rpf"))
        .unwrap_or(false);
    let original_game_path_blocked = looks_like_original_install(target_rpf);

    // Hash the target up front to prove (in plan mode) we never touched it.
    let target_sha256_before = if target_rpf_exists {
        fs::read(target_rpf).ok().map(|b| sha256_hex(&b))
    } else {
        None
    };

    if original_game_path_blocked {
        blocked_items.push(CodeWalkerTestRunBlockedItem {
            component: "target".to_string(),
            reason: "Target path looks like an original GTA V install; refusing the test run."
                .to_string(),
            block_type: "original_game_archive_suspected".to_string(),
        });
    }

    // ── Base URL ─────────────────────────────────────────────────────────────
    let resolved_base_url = base_url
        .map(|u| u.trim())
        .filter(|u| !u.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .to_string();
    let base_url_valid = base_url_is_valid(&resolved_base_url);
    if !base_url_valid {
        warnings.push(CodeWalkerTestRunWarning {
            code: "base_url_invalid".to_string(),
            message: format!("Base URL '{resolved_base_url}' is not a valid http(s) URL."),
        });
    }

    // ── Project dir present-or-creatable ─────────────────────────────────────
    let project_dir_ok = if project_dir.is_dir() {
        true
    } else if looks_like_original_install(project_dir) {
        false
    } else {
        match fs::create_dir_all(project_dir) {
            Ok(_) => true,
            Err(e) => {
                warnings.push(CodeWalkerTestRunWarning {
                    code: "project_dir_uncreatable".to_string(),
                    message: format!("Could not create project dir: {e}"),
                });
                false
            }
        }
    };

    // ── Required + optional input reports ────────────────────────────────────
    let mut inputs: Vec<CodeWalkerTestRunInputStatus> = Vec::new();
    let mut record = |name: &str, path: Option<&Path>, required: bool| -> (bool, bool) {
        match path {
            Some(p) => {
                let exists = p.is_file();
                let loaded = json_loadable(p);
                inputs.push(CodeWalkerTestRunInputStatus {
                    name: name.to_string(),
                    path: Some(p.display().to_string()),
                    required,
                    provided: true,
                    exists,
                    loaded,
                });
                (exists, loaded)
            }
            None => {
                inputs.push(CodeWalkerTestRunInputStatus {
                    name: name.to_string(),
                    path: None,
                    required,
                    provided: false,
                    exists: false,
                    loaded: false,
                });
                (false, false)
            }
        }
    };

    let (_, backup_loaded) = record("backup_report", Some(backup_report_path), true);
    let (_, readiness_loaded) = record("readiness_report", Some(readiness_report_path), true);
    let (_, entry_manifest_loaded) = record(
        "entry_manifest_report",
        Some(entry_manifest_report_path),
        true,
    );
    let (_, resolve_loaded) = record("resolve_report", Some(resolve_report_path), true);
    let (_, dry_replace_loaded) = record("dry_replace_plan", Some(dry_replace_plan_path), true);
    let (_, execution_gate_loaded) = record(
        "execution_gate_report",
        Some(execution_gate_report_path),
        true,
    );
    let (_, compat_loaded) = record(
        "compatibility_probe_report",
        compatibility_probe_report_path,
        false,
    );
    drop(record);

    let required_reports_loaded = backup_loaded
        && readiness_loaded
        && entry_manifest_loaded
        && resolve_loaded
        && dry_replace_loaded
        && execution_gate_loaded;

    let missing_required: Vec<String> = inputs
        .iter()
        .filter(|i| i.required && !i.loaded)
        .map(|i| i.name.clone())
        .collect();
    for name in &missing_required {
        blocked_items.push(CodeWalkerTestRunBlockedItem {
            component: "input".to_string(),
            reason: format!("Required input report '{name}' is missing or unparsable."),
            block_type: "required_report_unusable".to_string(),
        });
    }

    // ── Derive eligibility from the reports ──────────────────────────────────
    let (gate_view, _) = load_view::<ExecutionGateView>(execution_gate_report_path);
    let execution_gate_eligible = execution_gate_loaded && gate_view.codewalker_execution_eligible;
    let copied_test_archive_confirmed =
        execution_gate_loaded && gate_view.target_archive_classification == COPIED_TEST_ARCHIVE;

    let (plan_view, _) = load_view::<DryPlanView>(dry_replace_plan_path);
    let dry_replace_plan_has_planned_requests =
        dry_replace_loaded && !plan_view.planned_requests.is_empty();

    // Compat probe is optional. When provided it must not be *blocking* —
    // compatibleForSearch == Some(false) blocks; true/unknown is fine.
    let (compat_view, _) =
        load_view::<CompatProbeView>(compatibility_probe_report_path.unwrap_or(Path::new("")));
    let compatibility_probe_provided = compatibility_probe_report_path.is_some();
    let compatibility_probe_blocking =
        compatibility_probe_provided && compat_view.compatible_for_search == Some(false);
    let compatibility_probe_loaded_or_not_required = !compatibility_probe_provided || compat_loaded;

    let target_classification = classify_target(
        target_rpf,
        target_rpf_exists,
        target_rpf_extension_valid,
        copied_test_archive_confirmed,
    );
    let target_is_test_copy = copied_test_archive_confirmed
        && target_classification == CodeWalkerTargetArchiveClassification::CopiedTestArchive;

    // ── Authorization ────────────────────────────────────────────────────────
    let execution_requested = execute;
    let confirmation_phrase_provided = confirmation_phrase.is_some();
    let confirmation_phrase_matched = confirmation_phrase
        .map(|p| p == CONFIRMATION_PHRASE)
        .unwrap_or(false);

    // ── Eligibility gates (everything except execute flag + confirmation) ─────
    // `ready_for_execute` is true when all of these pass; execute mode additionally
    // requires the --execute flag and an exact confirmation phrase.
    let eligibility_passed = target_rpf_exists
        && target_rpf_extension_valid
        && !original_game_path_blocked
        && copied_test_archive_confirmed
        && project_dir_ok
        && required_reports_loaded
        && execution_gate_eligible
        && dry_replace_plan_has_planned_requests
        && compatibility_probe_loaded_or_not_required
        && !compatibility_probe_blocking
        && base_url_valid;
    let ready_for_execute = eligibility_passed;

    // ── Planned step sequence ────────────────────────────────────────────────
    let mut planned_steps = vec![
        step(
            1,
            "validate-inputs",
            "Validate all required inputs",
            "Verify the target is a copied test archive and load every required report.",
            false,
        ),
        step(
            2,
            "codewalker-replace-apply",
            "Controlled replace apply (copied test archive only)",
            "POST /api/replace-file for each planned request — requires execute + confirm. \
             MUTATES the copied archive.",
            true,
        ),
        step(
            3,
            "codewalker-post-write-verify",
            "Post-write verification",
            "Compare pre/post/backup hashes, classify the outcome, build a rollback plan.",
            false,
        ),
    ];

    // ── Execute mode ─────────────────────────────────────────────────────────
    let execute_allowed = execute && confirmation_phrase_matched && eligibility_passed;
    let mut codewalker_replace_apply_invoked = false;
    let mut post_write_verify_invoked = false;
    let mut replace_apply_report_path_out: Option<String> = None;
    let mut post_write_verify_report_path_out: Option<String> = None;
    let mut modifies_archive = false;
    let mut replace_succeeded = false;

    if execute && !execute_allowed {
        if !confirmation_phrase_matched {
            blocked_items.push(CodeWalkerTestRunBlockedItem {
                component: "authorization".to_string(),
                reason: format!(
                    "--execute requires the exact confirmation phrase: \"{CONFIRMATION_PHRASE}\"."
                ),
                block_type: "confirmation_phrase_required".to_string(),
            });
        }
        if !eligibility_passed {
            blocked_items.push(CodeWalkerTestRunBlockedItem {
                component: "authorization".to_string(),
                reason: "One or more eligibility gates failed; replace apply was not invoked."
                    .to_string(),
                block_type: "eligibility_gate_failed".to_string(),
            });
        }
    }

    if execute_allowed {
        // Step 2: invoke the existing scoped replace apply. It runs its own full
        // gate set and uses its OWN confirmation phrase.
        let apply_report = apply_codewalker_replace_on_test_archive(
            Some(&resolved_base_url),
            execution_gate_report_path,
            dry_replace_plan_path,
            true,
            Some(REPLACE_APPLY_PHRASE),
        )?;
        codewalker_replace_apply_invoked = true;
        planned_steps[1].executed = true;

        let apply_path = project_dir.join(REPLACE_APPLY_REPORT_NAME);
        match serde_json::to_string_pretty(&apply_report)
            .map_err(|e| e.to_string())
            .and_then(|s| fs::write(&apply_path, s).map_err(|e| e.to_string()))
        {
            Ok(_) => replace_apply_report_path_out = Some(apply_path.display().to_string()),
            Err(e) => warnings.push(CodeWalkerTestRunWarning {
                code: "replace_apply_report_write_failed".to_string(),
                message: format!("Failed to write replace apply report: {e}"),
            }),
        }

        // Re-read the outcome via the tolerant view for our own summary.
        let (outcome, _) = load_view::<ReplaceApplyOutcomeView>(&apply_path);
        modifies_archive = outcome.modifies_archive;
        replace_succeeded = outcome.replace_requests_sent && outcome.successful_replace_count > 0;

        // Step 3: post-write verification (read-only) using the apply report.
        if let Some(apply_path_str) = &replace_apply_report_path_out {
            let verify_report = build_codewalker_post_write_verify_report(
                target_rpf,
                Path::new(apply_path_str),
                backup_report_path,
                execution_gate_report_path,
                dry_replace_plan_path,
            )?;
            post_write_verify_invoked = true;
            planned_steps[2].executed = true;

            let verify_path = project_dir.join(POST_WRITE_VERIFY_REPORT_NAME);
            match serde_json::to_string_pretty(&verify_report)
                .map_err(|e| e.to_string())
                .and_then(|s| fs::write(&verify_path, s).map_err(|e| e.to_string()))
            {
                Ok(_) => {
                    post_write_verify_report_path_out = Some(verify_path.display().to_string())
                }
                Err(e) => warnings.push(CodeWalkerTestRunWarning {
                    code: "post_write_verify_report_write_failed".to_string(),
                    message: format!("Failed to write post-write verify report: {e}"),
                }),
            }
        }
    }

    // Mark step 1 executed whenever required inputs were validated.
    planned_steps[0].executed = required_reports_loaded;

    // Steps not executed are skipped; annotate mutating steps.
    for s in planned_steps.iter_mut() {
        if !s.executed {
            s.skipped = true;
            if s.mutates_archive {
                s.note = Some(
                    "Mutating step — copied test archive only; only runs with --execute and the \
                     exact confirm phrase once every gate passes."
                        .to_string(),
                );
            }
        }
    }
    let executed_steps: Vec<String> = planned_steps
        .iter()
        .filter(|s| s.executed)
        .map(|s| s.command_name.clone())
        .collect();
    let skipped_steps: Vec<String> = planned_steps
        .iter()
        .filter(|s| s.skipped)
        .map(|s| s.command_name.clone())
        .collect();

    // ── Confirm the target hash audit ────────────────────────────────────────
    let target_sha256_after = if target_rpf.is_file() {
        fs::read(target_rpf).ok().map(|b| sha256_hex(&b))
    } else {
        None
    };
    let target_hash_changed = match (&target_sha256_before, &target_sha256_after) {
        (Some(a), Some(b)) if a == b => CodeWalkerReplaceTargetHashChange::Unchanged,
        (Some(_), Some(_)) => CodeWalkerReplaceTargetHashChange::Changed,
        _ => CodeWalkerReplaceTargetHashChange::Unknown,
    };

    // In plan mode the target must be byte-identical.
    let plan_mode_target_unchanged = mode != CodeWalkerTestRunMode::ExecuteReplace
        && target_hash_changed == CodeWalkerReplaceTargetHashChange::Changed;
    if plan_mode_target_unchanged {
        blocked_items.push(CodeWalkerTestRunBlockedItem {
            component: "target".to_string(),
            reason: "Target SHA-256 changed during a plan-mode run — unexpected.".to_string(),
            block_type: "target_modified_in_plan_mode".to_string(),
        });
    }
    let plan_mode_no_http = mode != CodeWalkerTestRunMode::ExecuteReplace;
    let plan_mode_archive_not_modified = mode == CodeWalkerTestRunMode::ExecuteReplace
        || target_hash_changed != CodeWalkerReplaceTargetHashChange::Changed;

    // ── Gates ────────────────────────────────────────────────────────────────
    struct G {
        name: &'static str,
        passed: bool,
        blocking: bool,
        message: &'static str,
    }
    let gate_defs = vec![
        G {
            name: "target_rpf_present",
            passed: target_rpf_exists,
            blocking: true,
            message: "The target archive file exists.",
        },
        G {
            name: "target_rpf_extension_valid",
            passed: target_rpf_extension_valid,
            blocking: true,
            message: "The target archive has a .rpf extension.",
        },
        G {
            name: "target_not_original_game_archive",
            passed: !original_game_path_blocked,
            blocking: true,
            message: "The target path does not look like an original game install.",
        },
        G {
            name: "copied_test_archive_confirmed",
            passed: copied_test_archive_confirmed,
            blocking: true,
            message: "The execution gate classified the target as a copied test archive.",
        },
        G {
            name: "project_dir_present_or_creatable",
            passed: project_dir_ok,
            blocking: true,
            message: "The project dir exists or was created.",
        },
        G {
            name: "backup_report_loaded",
            passed: backup_loaded,
            blocking: true,
            message: "The backup report parsed successfully.",
        },
        G {
            name: "readiness_report_loaded",
            passed: readiness_loaded,
            blocking: true,
            message: "The readiness report parsed successfully.",
        },
        G {
            name: "entry_manifest_report_loaded",
            passed: entry_manifest_loaded,
            blocking: true,
            message: "The entry manifest report parsed successfully.",
        },
        G {
            name: "resolve_report_loaded",
            passed: resolve_loaded,
            blocking: true,
            message: "The resolve report parsed successfully.",
        },
        G {
            name: "dry_replace_plan_loaded",
            passed: dry_replace_loaded,
            blocking: true,
            message: "The dry replace plan parsed successfully.",
        },
        G {
            name: "execution_gate_report_loaded",
            passed: execution_gate_loaded,
            blocking: true,
            message: "The execution gate report parsed successfully.",
        },
        G {
            name: "compatibility_probe_loaded_or_not_required",
            passed: compatibility_probe_loaded_or_not_required,
            blocking: false,
            message: "The compatibility probe was loaded, or was not provided.",
        },
        G {
            name: "execution_gate_eligible",
            passed: execution_gate_eligible,
            blocking: true,
            message: "The execution gate reported codewalkerExecutionEligible true.",
        },
        G {
            name: "dry_replace_plan_has_planned_requests",
            passed: dry_replace_plan_has_planned_requests,
            blocking: true,
            message: "The dry replace plan has at least one planned request.",
        },
        G {
            name: "plan_only_no_http_requests",
            passed: plan_mode_no_http || execute,
            blocking: false,
            message: "Plan mode sent no HTTP request of any kind.",
        },
        G {
            name: "plan_only_archive_not_modified",
            passed: plan_mode_archive_not_modified,
            blocking: false,
            message: "Plan mode did not modify the target archive.",
        },
        G {
            name: "execute_requires_confirmation",
            passed: !execute || confirmation_phrase_matched,
            blocking: false,
            message: "Execute mode was either not requested or matched the confirmation phrase.",
        },
        G {
            name: "replace_apply_invoked_only_when_allowed",
            passed: !codewalker_replace_apply_invoked || execute_allowed,
            blocking: false,
            message: "Replace apply was only invoked when execute was allowed.",
        },
        G {
            name: "post_write_verify_invoked_after_execute",
            passed: !codewalker_replace_apply_invoked || post_write_verify_invoked,
            blocking: false,
            message: "Post-write verification ran after every replace apply.",
        },
        G {
            name: "rollback_not_automatic",
            passed: true,
            blocking: false,
            message: "Rollback restore was never invoked automatically.",
        },
        G {
            name: "null_adapter_still_active",
            passed: null_adapter_active,
            blocking: false,
            message: "The active adapter remains NullRpfAdapter.",
        },
        G {
            name: "native_parser_not_used",
            passed: true,
            blocking: false,
            message: "No native RPF parsing was performed.",
        },
        G {
            name: "original_paths_blocked",
            passed: !original_game_path_blocked,
            blocking: true,
            message: "Original game archive paths are blocked.",
        },
    ];

    let gates: Vec<CodeWalkerTestRunSafetyGate> = gate_defs
        .iter()
        .map(|g| {
            let severity = if g.passed {
                CodeWalkerApiSeverity::Info
            } else if g.blocking {
                CodeWalkerApiSeverity::Blocking
            } else {
                CodeWalkerApiSeverity::Warning
            };
            CodeWalkerTestRunSafetyGate {
                name: g.name.to_string(),
                passed: g.passed,
                severity,
                message: g.message.to_string(),
            }
        })
        .collect();

    // ── Standing blocks ──────────────────────────────────────────────────────
    blocked_items.push(CodeWalkerTestRunBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerTestRunBlockedItem {
        component: "writer".to_string(),
        reason: "Global RPF writing remains disabled; execution is scoped to copied test archives."
            .to_string(),
        block_type: "global_writer_disabled".to_string(),
    });

    // ── Status ───────────────────────────────────────────────────────────────
    let inputs_unusable = !target_rpf_exists
        || (target_rpf_exists && !target_rpf_extension_valid)
        || !required_reports_loaded;
    let status = if execute {
        if !execute_allowed {
            if inputs_unusable {
                CodeWalkerTestRunStatus::InvalidInput
            } else {
                CodeWalkerTestRunStatus::Blocked
            }
        } else if replace_succeeded {
            CodeWalkerTestRunStatus::Executed
        } else {
            CodeWalkerTestRunStatus::ExecuteFailed
        }
    } else if inputs_unusable {
        CodeWalkerTestRunStatus::InvalidInput
    } else if ready_for_execute {
        CodeWalkerTestRunStatus::PlannedReady
    } else {
        CodeWalkerTestRunStatus::PlannedNotReady
    };

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();
    let executed_step_count = executed_steps.len();
    let skipped_step_count = skipped_steps.len();

    let summary = CodeWalkerTestRunSummary {
        total_gates: gates.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        planned_step_count: planned_steps.len(),
        executed_step_count,
        skipped_step_count,
        ready_for_execute,
        modifies_archive,
        execution_requested,
    };

    Ok(CodeWalkerTestRunReport {
        status,
        mode,
        target_rpf: target_rpf.display().to_string(),
        target_rpf_exists,
        target_rpf_extension_valid,
        target_classification,
        target_is_test_copy,
        original_game_path_blocked,
        base_url: resolved_base_url,
        project_dir: project_dir.display().to_string(),
        backup_report_path: backup_report_path.display().to_string(),
        readiness_report_path: readiness_report_path.display().to_string(),
        entry_manifest_report_path: entry_manifest_report_path.display().to_string(),
        resolve_report_path: resolve_report_path.display().to_string(),
        dry_replace_plan_path: dry_replace_plan_path.display().to_string(),
        execution_gate_report_path: execution_gate_report_path.display().to_string(),
        compatibility_probe_report_path: compatibility_probe_report_path
            .map(|p| p.display().to_string()),
        replace_apply_report_path: replace_apply_report_path_out,
        post_write_verify_report_path: post_write_verify_report_path_out,
        inputs,
        planned_steps,
        executed_steps,
        skipped_steps,
        execution_requested,
        confirmation_phrase_provided,
        confirmation_phrase_matched,
        expected_confirmation_phrase: CONFIRMATION_PHRASE.to_string(),
        execution_gate_eligible,
        copied_test_archive_confirmed,
        dry_replace_plan_has_planned_requests,
        compatibility_probe_blocking,
        ready_for_execute,
        codewalker_replace_apply_invoked,
        post_write_verify_invoked,
        rollback_restore_invoked: false,
        target_sha256_before,
        target_sha256_after,
        target_hash_changed,
        modifies_archive,
        writer_allowed_global: false,
        null_adapter_active,
        native_parser_used: false,
        external_tool_executed: false,
        active_adapter_name,
        gates,
        warnings,
        blocked_items,
        summary,
    })
}
