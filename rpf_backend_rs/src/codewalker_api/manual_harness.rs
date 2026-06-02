use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// The exact phrase required before execute mode is even acknowledged.
pub const CONFIRMATION_PHRASE: &str = "I understand this will run the copied test archive harness";

/// Default planned CodeWalker.API base URL.
const DEFAULT_BASE_URL: &str = "http://localhost:5555";

/// Fallback safe output directory for a generated script when no project dir is given.
const FALLBACK_SCRIPT_DIR: &str = ".tmp";

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

/// Same conservative classification used by the execution gate / rollback.
fn classify_target(
    target: &Path,
    exists: bool,
    extension_valid: bool,
    target_is_test_copy: bool,
) -> CodeWalkerTargetArchiveClassification {
    // Conservative: an original game path is refused even if the file is absent.
    if looks_like_original_install(target) {
        return CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected;
    }
    if !exists {
        return CodeWalkerTargetArchiveClassification::Missing;
    }
    if !extension_valid {
        return CodeWalkerTargetArchiveClassification::InvalidExtension;
    }
    if target_is_test_copy {
        CodeWalkerTargetArchiveClassification::CopiedTestArchive
    } else {
        CodeWalkerTargetArchiveClassification::UnknownArchive
    }
}

/// A base URL is acceptable when it starts with http:// or https:// and has a host.
fn base_url_is_valid(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("http://") && u.len() > "http://".len())
        || (u.starts_with("https://") && u.len() > "https://".len())
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerManualHarnessSafetyGate {
    CodeWalkerManualHarnessSafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

fn opt_path_string(p: Option<&Path>) -> Option<String> {
    p.map(|p| p.display().to_string())
}

/// Build (and optionally write a script for) a safe real copied-archive manual
/// test harness. See [`CodeWalkerManualHarnessReport`] for the contract.
///
/// In plan/generate-script mode this NEVER calls CodeWalker, sends NO HTTP request,
/// never uses POST, executes no external tool, parses no RPF internals, and never
/// modifies the target archive. Original game install paths are blocked. Even when
/// `execute` is requested and confirmed, this milestone keeps `execution_performed`
/// `false` and performs no automatic full execution.
#[allow(clippy::too_many_arguments)]
pub fn build_codewalker_manual_test_harness(
    target_rpf: &Path,
    base_url: Option<&str>,
    target_is_test_copy: bool,
    project_dir: Option<&Path>,
    bundle_dir: Option<&Path>,
    generate_script: bool,
    execute: bool,
    confirmation_phrase: Option<&str>,
) -> Result<CodeWalkerManualHarnessReport, String> {
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerManualHarnessWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerManualHarnessBlockedItem> = Vec::new();

    // ── Target facts ─────────────────────────────────────────────────────────
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
    let original_game_path_blocked =
        classification == CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected;

    // Hash the target up front so we can prove we never touched it.
    let target_sha256_before = if target_rpf_exists {
        fs::read(target_rpf).ok().map(|b| sha256_hex(&b))
    } else {
        None
    };

    if original_game_path_blocked {
        blocked_items.push(CodeWalkerManualHarnessBlockedItem {
            component: "target".to_string(),
            reason: "Target path looks like an original GTA V install; refusing harness."
                .to_string(),
            block_type: "original_game_archive_suspected".to_string(),
        });
    }
    if !target_is_test_copy {
        blocked_items.push(CodeWalkerManualHarnessBlockedItem {
            component: "target".to_string(),
            reason: "Target was not confirmed as a copied test archive (--target-is-test-copy)."
                .to_string(),
            block_type: "target_not_confirmed_test_copy".to_string(),
        });
    }
    if !target_rpf_exists {
        blocked_items.push(CodeWalkerManualHarnessBlockedItem {
            component: "target".to_string(),
            reason: "Target archive file does not exist.".to_string(),
            block_type: "target_missing".to_string(),
        });
    }
    if target_rpf_exists && !target_rpf_extension_valid {
        blocked_items.push(CodeWalkerManualHarnessBlockedItem {
            component: "target".to_string(),
            reason: "Target file does not have a .rpf extension.".to_string(),
            block_type: "target_extension_invalid".to_string(),
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
        warnings.push(CodeWalkerManualHarnessWarning {
            code: "base_url_invalid".to_string(),
            message: format!("Base URL '{resolved_base_url}' is not a valid http(s) URL."),
        });
    }

    // ── Optional inputs ──────────────────────────────────────────────────────
    let bundle_dir_path = bundle_dir.map(|p| p.to_path_buf());
    let patch_plan_path = bundle_dir.map(|b| b.join("patch_plan.json"));
    let entry_manifest_report = project_dir.map(|p| p.join("entry_manifest_report.json"));
    let dry_replace_plan_report = project_dir.map(|p| p.join("dry_replace_plan.json"));
    let execution_gate_report = project_dir.map(|p| p.join("execution_gate_report.json"));
    let backup_report = project_dir.map(|p| p.join("backup_report.json"));

    let mut inputs: Vec<CodeWalkerManualHarnessInput> = Vec::new();
    let mut record_input = |name: &str, path: &Option<PathBuf>| -> bool {
        match path {
            Some(p) => {
                let exists = p.is_file() || p.is_dir();
                inputs.push(CodeWalkerManualHarnessInput {
                    name: name.to_string(),
                    path: Some(p.display().to_string()),
                    provided: true,
                    exists,
                });
                exists
            }
            None => {
                inputs.push(CodeWalkerManualHarnessInput {
                    name: name.to_string(),
                    path: None,
                    provided: false,
                    exists: false,
                });
                false
            }
        }
    };
    let bundle_dir_ok = record_input("bundle_dir", &bundle_dir_path);
    let patch_plan_ok = record_input("patch_plan", &patch_plan_path);
    let entry_manifest_ok = record_input("entry_manifest_report", &entry_manifest_report);
    let dry_replace_ok = record_input("dry_replace_plan_report", &dry_replace_plan_report);
    let execution_gate_ok = record_input("execution_gate_report", &execution_gate_report);
    let backup_report_ok = record_input("backup_report", &backup_report);

    let missing_inputs: Vec<&str> = inputs
        .iter()
        .filter(|i| i.provided && !i.exists)
        .map(|i| i.name.as_str())
        .collect();
    let missing_input_count = missing_inputs.len();
    if missing_input_count > 0 {
        warnings.push(CodeWalkerManualHarnessWarning {
            code: "missing_inputs".to_string(),
            message: format!(
                "Some referenced inputs were not found yet: {}. The plan is still generated; \
                 produce these via the listed steps before execution.",
                missing_inputs.join(", ")
            ),
        });
    }

    // ── Planned step sequence (the full real copied-test flow) ───────────────
    let proj = project_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".tmp/real_codewalker_test"));
    let proj_disp = proj.display().to_string();
    let target_disp = target_rpf.display().to_string();
    let bundle_disp = bundle_dir
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<bundle-dir>".to_string());

    struct StepDef {
        command: &'static str,
        title: &'static str,
        description: &'static str,
        required: Vec<&'static str>,
        mutates: bool,
    }
    let step_defs = vec![
        StepDef {
            command: "probe-rpf",
            title: "Read-only target probe",
            description: "Read target metadata + SHA-256. No RPF internals parsed.",
            required: vec![],
            mutates: false,
        },
        StepDef {
            command: "backup-rpf",
            title: "Hash-verified backup",
            description: "Copy the copied test archive into a backup dir and verify by SHA-256.",
            required: vec![],
            mutates: false,
        },
        StepDef {
            command: "codewalker-detect",
            title: "Detect CodeWalker.API",
            description: "Read-only GET detection of a local CodeWalker.API.",
            required: vec![],
            mutates: false,
        },
        StepDef {
            command: "codewalker-readiness",
            title: "Readiness probe",
            description: "Read-only readiness/health probe of CodeWalker.API.",
            required: vec![],
            mutates: false,
        },
        StepDef {
            command: "rpf-entry-manifest",
            title: "Build entry manifest",
            description: "Build the entry manifest from the exported bundle.",
            required: vec!["bundle_dir"],
            mutates: false,
        },
        StepDef {
            command: "codewalker-resolve-targets",
            title: "Resolve targets",
            description: "Resolve manifest entries to CodeWalker targets (safe GET search).",
            required: vec!["entry_manifest_report"],
            mutates: false,
        },
        StepDef {
            command: "codewalker-dry-replace-plan",
            title: "Dry replace plan",
            description: "Model /api/replace-file payloads. No HTTP, dry-run only.",
            required: vec!["bundle_dir", "entry_manifest_report"],
            mutates: false,
        },
        StepDef {
            command: "writer-permission",
            title: "Manual writer permission",
            description: "Produce the manual writer-permission token + confirmation.",
            required: vec![],
            mutates: false,
        },
        StepDef {
            command: "codewalker-execution-gate",
            title: "Execution gate",
            description: "Decide whether a future replace would be eligible. No execution.",
            required: vec![
                "dry_replace_plan_report",
                "entry_manifest_report",
                "backup_report",
            ],
            mutates: false,
        },
        StepDef {
            command: "codewalker-replace-apply",
            title: "Controlled replace apply (copied test archive only)",
            description: "POST /api/replace-file for the copied test archive only. Requires \
                          --execute and the exact confirm phrase. MUTATES the copied archive.",
            required: vec!["execution_gate_report", "dry_replace_plan_report"],
            mutates: true,
        },
        StepDef {
            command: "codewalker-post-write-verify",
            title: "Post-write verification",
            description: "Compare hashes, classify outcome, build a rollback plan.",
            required: vec!["backup_report", "execution_gate_report"],
            mutates: false,
        },
        StepDef {
            command: "codewalker-rollback-restore",
            title: "Optional rollback restore",
            description: "Restore the copied test archive from the verified backup if needed. \
                          MUTATES the copied archive.",
            required: vec!["backup_report"],
            mutates: true,
        },
    ];

    let input_available = |name: &str| -> bool {
        match name {
            "bundle_dir" => bundle_dir_ok,
            "patch_plan" => patch_plan_ok,
            "entry_manifest_report" => entry_manifest_ok,
            "dry_replace_plan_report" => dry_replace_ok,
            "execution_gate_report" => execution_gate_ok,
            "backup_report" => backup_report_ok,
            _ => false,
        }
    };

    let planned_steps: Vec<CodeWalkerManualHarnessStep> = step_defs
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let inputs_available = d.required.iter().all(|r| input_available(r));
            let note = if d.mutates {
                Some(
                    "Mutating step — copied test archive only; gated by explicit execute + \
                     confirm phrase. Never run against an original game archive."
                        .to_string(),
                )
            } else if !inputs_available {
                Some("Required input(s) not present yet; complete earlier steps first.".to_string())
            } else {
                None
            };
            CodeWalkerManualHarnessStep {
                index: i + 1,
                command_name: d.command.to_string(),
                title: d.title.to_string(),
                description: d.description.to_string(),
                required_inputs: d.required.iter().map(|s| s.to_string()).collect(),
                inputs_available,
                mutates_archive: d.mutates,
                note,
            }
        })
        .collect();

    // ── Generated command checklist ──────────────────────────────────────────
    let cargo = "cargo run --manifest-path rpf_backend_rs/Cargo.toml --";
    let confirm_phrase_for_cmd = CONFIRMATION_PHRASE;
    let generated_commands: Vec<String> = vec![
        format!("{cargo} probe-rpf --target-rpf \"{target_disp}\" --out {proj_disp}/probe_report.json"),
        format!(
            "{cargo} backup-rpf --target-rpf \"{target_disp}\" --backup-dir {proj_disp}/backup --out {proj_disp}/backup_report.json"
        ),
        format!("{cargo} codewalker-detect --base-url {resolved_base_url} --out {proj_disp}/detect_report.json"),
        format!("{cargo} codewalker-readiness --base-url {resolved_base_url} --out {proj_disp}/readiness_report.json"),
        format!("{cargo} rpf-entry-manifest --bundle-dir {bundle_disp} --out {proj_disp}/entry_manifest_report.json"),
        format!(
            "{cargo} codewalker-resolve-targets --entry-manifest-report {proj_disp}/entry_manifest_report.json --base-url {resolved_base_url} --out {proj_disp}/resolve_report.json"
        ),
        format!(
            "{cargo} codewalker-dry-replace-plan --bundle-dir {bundle_disp} --entry-manifest-report {proj_disp}/entry_manifest_report.json --resolve-report {proj_disp}/resolve_report.json --out {proj_disp}/dry_replace_plan.json"
        ),
        format!("{cargo} writer-permission --confirm \"<writer permission phrase>\" --out {proj_disp}/permission_report.json"),
        format!(
            "{cargo} codewalker-execution-gate --target-rpf \"{target_disp}\" --target-is-test-copy --dry-replace-plan {proj_disp}/dry_replace_plan.json --permission-report {proj_disp}/permission_report.json --readiness-report {proj_disp}/readiness_report.json --entry-manifest-report {proj_disp}/entry_manifest_report.json --backup-report {proj_disp}/backup_report.json --out {proj_disp}/execution_gate_report.json"
        ),
        format!(
            "# MUTATING — uncomment + confirm manually: {cargo} codewalker-replace-apply --base-url {resolved_base_url} --execution-gate-report {proj_disp}/execution_gate_report.json --dry-replace-plan {proj_disp}/dry_replace_plan.json --execute --confirm \"<replace apply phrase>\" --out {proj_disp}/replace_apply_report.json"
        ),
        format!(
            "{cargo} codewalker-post-write-verify --target-rpf \"{target_disp}\" --replace-apply-report {proj_disp}/replace_apply_report.json --backup-report {proj_disp}/backup_report.json --execution-gate-report {proj_disp}/execution_gate_report.json --dry-replace-plan {proj_disp}/dry_replace_plan.json --out {proj_disp}/post_write_verify_report.json"
        ),
        format!(
            "# MUTATING — uncomment + confirm manually: {cargo} codewalker-rollback-restore --target-rpf \"{target_disp}\" --post-write-verify-report {proj_disp}/post_write_verify_report.json --backup-report {proj_disp}/backup_report.json --execute-rollback --confirm \"<rollback phrase>\""
        ),
    ];

    // ── Mode + authorization ─────────────────────────────────────────────────
    let execute_requested = execute;
    let confirmation_phrase_provided = confirmation_phrase.is_some();
    let confirmation_phrase_matched = confirmation_phrase
        .map(|p| p == CONFIRMATION_PHRASE)
        .unwrap_or(false);

    let mode = if execute_requested {
        CodeWalkerManualHarnessMode::ExecuteExistingPipeline
    } else if generate_script {
        CodeWalkerManualHarnessMode::GenerateScript
    } else {
        CodeWalkerManualHarnessMode::PlanOnly
    };

    if execute_requested {
        if !confirmation_phrase_matched {
            blocked_items.push(CodeWalkerManualHarnessBlockedItem {
                component: "authorization".to_string(),
                reason: format!(
                    "--execute requires the exact confirmation phrase: \"{CONFIRMATION_PHRASE}\"."
                ),
                block_type: "confirmation_phrase_required".to_string(),
            });
        }
        // This milestone never performs automatic full execution.
        warnings.push(CodeWalkerManualHarnessWarning {
            code: "automatic_full_execution_not_enabled".to_string(),
            message: "Automatic full execution is not enabled in this milestone. Run the listed \
                      commands manually, reviewing each gate report before any mutating step."
                .to_string(),
        });
    }

    // ── Optional safe script generation ──────────────────────────────────────
    let target_ok_for_plan = target_rpf_exists
        && target_rpf_extension_valid
        && target_is_test_copy
        && !original_game_path_blocked;

    let mut generated_script_path: Option<String> = None;
    let mut script_generated = false;
    if generate_script {
        if !target_ok_for_plan {
            warnings.push(CodeWalkerManualHarnessWarning {
                code: "script_not_generated".to_string(),
                message: "Script not generated because the target failed a blocking safety gate."
                    .to_string(),
            });
        } else {
            // Choose a clearly-safe output directory: the provided project dir, else .tmp.
            let script_dir = project_dir
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(FALLBACK_SCRIPT_DIR));
            if looks_like_original_install(&script_dir) {
                warnings.push(CodeWalkerManualHarnessWarning {
                    code: "script_dir_unsafe".to_string(),
                    message: "Refusing to write a script under a path resembling a game install."
                        .to_string(),
                });
            } else {
                let script_path = script_dir.join("codewalker_manual_harness.ps1");
                let body = render_script(
                    &target_disp,
                    &resolved_base_url,
                    confirm_phrase_for_cmd,
                    &generated_commands,
                );
                let write_result =
                    fs::create_dir_all(&script_dir).and_then(|_| fs::write(&script_path, body));
                match write_result {
                    Ok(_) => {
                        script_generated = true;
                        generated_script_path = Some(script_path.display().to_string());
                    }
                    Err(e) => warnings.push(CodeWalkerManualHarnessWarning {
                        code: "script_write_failed".to_string(),
                        message: format!("Failed to write script: {e}"),
                    }),
                }
            }
        }
    }

    // ── Confirm the target was never modified ────────────────────────────────
    let target_sha256_after = if target_rpf_exists {
        fs::read(target_rpf).ok().map(|b| sha256_hex(&b))
    } else {
        None
    };
    let target_unchanged = target_sha256_before == target_sha256_after;
    if !target_unchanged {
        blocked_items.push(CodeWalkerManualHarnessBlockedItem {
            component: "target".to_string(),
            reason: "Target SHA-256 changed during the harness run — unexpected.".to_string(),
            block_type: "target_modified_unexpectedly".to_string(),
        });
    }

    // ── Gates ────────────────────────────────────────────────────────────────
    let plan_generated = !planned_steps.is_empty() && !generated_commands.is_empty();
    let script_gate_ok = if generate_script {
        script_generated || !target_ok_for_plan
    } else {
        generated_script_path.is_none()
    };
    let execute_gate_ok = !execute_requested || confirmation_phrase_matched;

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
            name: "target_marked_as_test_copy",
            passed: target_is_test_copy,
            blocking: true,
            message: "The target was explicitly marked/confirmed as a test copy.",
        },
        G {
            name: "target_not_original_game_archive",
            passed: !original_game_path_blocked,
            blocking: true,
            message: "The target path does not look like an original game install.",
        },
        G {
            name: "target_path_allowed_for_test_execution",
            passed: target_path_allowed_for_test_execution,
            blocking: true,
            message: "The target classified as a copied test archive.",
        },
        G {
            name: "base_url_valid",
            passed: base_url_valid,
            blocking: false,
            message: "The base URL is a valid http(s) URL.",
        },
        G {
            name: "plan_generated",
            passed: plan_generated,
            blocking: false,
            message: "A plan and command checklist were generated.",
        },
        G {
            name: "script_generated_only_if_requested",
            passed: script_gate_ok,
            blocking: false,
            message: "A script file was written only when --generate-script was requested.",
        },
        G {
            name: "execute_not_requested_or_confirmed",
            passed: execute_gate_ok,
            blocking: false,
            message: "Execution was either not requested or matched the confirmation phrase.",
        },
        G {
            name: "codewalker_not_called_in_plan_mode",
            passed: true,
            blocking: false,
            message: "CodeWalker was not called.",
        },
        G {
            name: "no_http_requests_in_plan_mode",
            passed: true,
            blocking: false,
            message: "No HTTP request of any kind was sent.",
        },
        G {
            name: "no_external_tool_executed",
            passed: true,
            blocking: false,
            message: "No external tool was executed.",
        },
        G {
            name: "native_parser_not_used",
            passed: true,
            blocking: false,
            message: "No native RPF parsing was performed.",
        },
        G {
            name: "archive_not_modified_in_plan_mode",
            passed: target_unchanged,
            blocking: false,
            message: "The target archive was not modified.",
        },
    ];

    let gates: Vec<CodeWalkerManualHarnessSafetyGate> = gate_defs
        .iter()
        .map(|g| {
            let severity = if g.passed {
                CodeWalkerApiSeverity::Info
            } else if g.blocking {
                CodeWalkerApiSeverity::Blocking
            } else {
                CodeWalkerApiSeverity::Warning
            };
            gate(g.name, g.passed, severity, g.message)
        })
        .collect();

    // ── Standing block: real writing remains disabled ────────────────────────
    blocked_items.push(CodeWalkerManualHarnessBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerManualHarnessBlockedItem {
        component: "codewalker".to_string(),
        reason: "Automatic full CodeWalker execution is not enabled in this milestone.".to_string(),
        block_type: "automatic_execution_not_enabled".to_string(),
    });

    // ── Status ───────────────────────────────────────────────────────────────
    let blocking_failed = gate_defs.iter().any(|g| g.blocking && !g.passed);
    let status = if !target_rpf_exists || (target_rpf_exists && !target_rpf_extension_valid) {
        CodeWalkerManualHarnessStatus::InvalidInput
    } else if blocking_failed {
        CodeWalkerManualHarnessStatus::Blocked
    } else if execute_requested {
        CodeWalkerManualHarnessStatus::ExecuteRequestedNotPerformed
    } else if script_generated {
        CodeWalkerManualHarnessStatus::ScriptGenerated
    } else {
        CodeWalkerManualHarnessStatus::Planned
    };

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerManualHarnessSummary {
        total_gates: gates.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        planned_step_count: planned_steps.len(),
        generated_command_count: generated_commands.len(),
        missing_input_count,
        script_generated,
        execution_performed: false,
        modifies_archive: false,
    };

    Ok(CodeWalkerManualHarnessReport {
        status,
        mode,
        base_url: resolved_base_url,
        target_rpf: target_disp,
        target_rpf_exists,
        target_rpf_extension_valid,
        target_classification: classification,
        target_marked_as_test_copy: target_is_test_copy,
        target_path_allowed_for_test_execution,
        original_game_path_blocked,
        target_sha256_before,
        target_sha256_after,
        project_dir: opt_path_string(project_dir),
        bundle_dir: opt_path_string(bundle_dir),
        patch_plan_path: patch_plan_path
            .as_deref()
            .and_then(|p| opt_path_string(Some(p))),
        entry_manifest_report: entry_manifest_report
            .as_deref()
            .and_then(|p| opt_path_string(Some(p))),
        dry_replace_plan_report: dry_replace_plan_report
            .as_deref()
            .and_then(|p| opt_path_string(Some(p))),
        execution_gate_report: execution_gate_report
            .as_deref()
            .and_then(|p| opt_path_string(Some(p))),
        backup_report: backup_report
            .as_deref()
            .and_then(|p| opt_path_string(Some(p))),
        inputs,
        planned_steps,
        generated_commands,
        generated_script_path,
        execute_requested,
        confirmation_phrase_provided,
        confirmation_phrase_matched,
        expected_confirmation_phrase: CONFIRMATION_PHRASE.to_string(),
        execution_performed: false,
        codewalker_called: false,
        http_requests_sent: false,
        post_requests_sent: false,
        modifies_archive: false,
        native_parser_used: false,
        external_tool_executed: false,
        writer_allowed: false,
        active_adapter_name,
        null_adapter_active,
        gates,
        warnings,
        blocked_items,
        summary,
    })
}

/// Render a safe PowerShell checklist/script. Defaults to plan/check mode; the two
/// mutating commands stay commented out behind explicit placeholders the user must
/// fill in and uncomment by hand.
fn render_script(
    target: &str,
    base_url: &str,
    confirm_phrase: &str,
    commands: &[String],
) -> String {
    let mut s = String::new();
    s.push_str("# CodeWalker manual test harness — GENERATED CHECKLIST (T0.6.8)\n");
    s.push_str("# Safe by default: this script PRINTS the manual command checklist.\n");
    s.push_str("# It performs NO archive mutation on its own. Mutating commands are\n");
    s.push_str("# commented out and require you to fill placeholders + uncomment them.\n");
    s.push_str("#\n");
    s.push_str("# COPIED TEST ARCHIVES ONLY. Never point this at an original game archive.\n");
    s.push_str(&format!("# Target:   {target}\n"));
    s.push_str(&format!("# Base URL: {base_url}\n"));
    s.push_str(&format!("# Execute confirm phrase: {confirm_phrase}\n"));
    s.push_str("\n$ErrorActionPreference = 'Stop'\n");
    s.push_str("Write-Host 'CodeWalker manual test harness checklist (review each step):'\n\n");
    for (i, cmd) in commands.iter().enumerate() {
        s.push_str(&format!("# Step {}\n", i + 1));
        if cmd.trim_start().starts_with('#') {
            // Already a commented mutating step — keep it commented in the script.
            s.push_str(&format!("Write-Host '{}'\n", escape_single(cmd)));
            s.push_str(&format!("{cmd}\n\n"));
        } else {
            s.push_str(&format!("Write-Host '{}'\n", escape_single(cmd)));
            s.push_str(&format!("# {cmd}\n\n"));
        }
    }
    s.push_str(
        "Write-Host 'Done. Run the printed commands manually, reviewing every gate report.'\n",
    );
    s
}

fn escape_single(s: &str) -> String {
    s.replace('\'', "''")
}
