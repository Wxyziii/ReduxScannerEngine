use super::editor_contract::*;
use super::editor_result::*;
use super::report::*;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

// ── Internal evaluation types ────────────────────────────────────────────────

struct BlockItem {
    reason: String,
    block_type: &'static str,
}

struct OperationEvaluation {
    operation_id: String,
    file_path: String,
    ok: bool,
    would_change: bool,
    validators_planned: Vec<String>,
    block_items: Vec<BlockItem>,
    warnings: Vec<String>,
}

// ── Core evaluator (single source of truth) ──────────────────────────────────

fn evaluate_operation(op: &EditorOperation) -> OperationEvaluation {
    let mut eval = OperationEvaluation {
        operation_id: op.id.clone(),
        file_path: op.path.clone(),
        ok: true,
        would_change: true,
        validators_planned: Vec::new(),
        block_items: Vec::new(),
        warnings: Vec::new(),
    };

    // 1. Phase check
    if op.phase != "first_patch" {
        eval.ok = false;
        eval.block_items.push(BlockItem {
            reason: format!(
                "Operation phase '{}' not supported in first_controlled_patch mode",
                op.phase
            ),
            block_type: "invalid_phase",
        });
    }

    // 2. Scope check
    let file_name = Path::new(&op.path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| op.path.clone());

    const ALLOWED_FIRST_PATCH: &[&str] = &[
        "visualsettings.dat",
        "cloudkeyframes.xml",
        "timecycle_mods_1.xml",
    ];
    const BLOCKED_OR_DEFERRED: &[&str] = &[
        "weather.xml",
        "timecycle_mods_3.xml",
        "timecycle_mods_4.xml",
        "w_foggy.xml",
        "w_clouds.xml",
    ];

    if !ALLOWED_FIRST_PATCH.contains(&file_name.as_str()) {
        eval.ok = false;
        if BLOCKED_OR_DEFERRED.contains(&file_name.as_str()) {
            eval.block_items.push(BlockItem {
                reason: format!("Blocked/Deferred file targeted: {}", op.path),
                block_type: "blocked_deferred",
            });
        } else if is_binary(&op.path) {
            eval.block_items.push(BlockItem {
                reason: format!("Binary file targeted: {}", op.path),
                block_type: "binary_file",
            });
        } else if is_rpf(&op.path) {
            eval.block_items.push(BlockItem {
                reason: format!("RPF archive targeted: {}", op.path),
                block_type: "rpf_archive",
            });
        } else if is_unrelated_component(&op.path) {
            eval.block_items.push(BlockItem {
                reason: format!("Unrelated component file targeted: {}", op.path),
                block_type: "unrelated_component",
            });
        } else {
            eval.block_items.push(BlockItem {
                reason: format!("File not in allowed first_patch list: {}", op.path),
                block_type: "not_in_scope",
            });
        }
    }

    // 3. Tool check
    const VALID_TOOLS: &[&str] = &[
        "dat_named_key_editor",
        "xml_cloudkeyframe_editor",
        "xml_timecycle_editor",
        "text_file_editor",
    ];
    if !VALID_TOOLS.contains(&op.tool.as_str()) {
        eval.ok = false;
        eval.block_items.push(BlockItem {
            reason: format!("Unknown tool: {}", op.tool),
            block_type: "unknown_tool",
        });
    }

    // 4. Operation type check
    const VALID_OP_TYPES: &[&str] = &[
        "dat_named_key_candidate",
        "xml_color_like_candidate",
        "xml_color_like_adjustment",
        "text_replace",
        "text_append",
        "text_prepend",
    ];
    if !VALID_OP_TYPES.contains(&op.op_type.as_str()) {
        eval.ok = false;
        eval.block_items.push(BlockItem {
            reason: format!("Unknown operationType: {}", op.op_type),
            block_type: "unknown_op_type",
        });
    }

    // 5. Validation required check
    if op.validation_required.is_empty() {
        eval.ok = false;
        eval.block_items.push(BlockItem {
            reason: "Missing validationRequired array".to_string(),
            block_type: "missing_validation",
        });
    }

    // 6. Intent check (hypothesis wording required)
    let intent_lower = op.intent.to_lowercase();
    if !intent_lower.contains("hypothesis")
        && !intent_lower.contains("believe")
        && !intent_lower.contains("likely")
    {
        eval.ok = false;
        eval.block_items.push(BlockItem {
            reason: "Operation intent missing hypothesis wording (hypothesis, believe, likely)"
                .to_string(),
            block_type: "invalid_intent",
        });
    }

    // 7. Plan validators (only populated for allowed operations)
    if eval.ok {
        eval.validators_planned
            .push("scanner_scope_validator".to_string());
        if op.tool == "text_file_editor" {
            eval.validators_planned
                .push("text_content_validator".to_string());
        } else if file_name.ends_with(".xml") {
            eval.validators_planned
                .push("xml_validator_parse".to_string());
            eval.validators_planned
                .push("xml_validator_color_like_only".to_string());
            eval.validators_planned
                .push("xml_validator_no_node_deletion".to_string());
        } else if file_name.ends_with(".dat") {
            eval.validators_planned
                .push("dat_validator_parse".to_string());
            eval.validators_planned
                .push("dat_validator_named_key".to_string());
        }
        eval.validators_planned
            .push("in_game_check_placeholder".to_string());
    }

    eval
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns a legacy `EditorBatchResult` (used by the `editor-dry-run` command).
pub fn execute_dry_run(
    patch_plan_path: &Path,
    target_operation_id: Option<&str>,
) -> Result<EditorBatchResult> {
    let content = fs::read_to_string(patch_plan_path)
        .with_context(|| format!("Failed to read patch plan: {}", patch_plan_path.display()))?;

    let plan: EditorPlan =
        serde_json::from_str(&content).with_context(|| "Failed to parse patch plan JSON")?;

    let mut results = Vec::new();
    let mut batch_ok = true;
    let mut batch_errors = Vec::new();

    for op in &plan.operations {
        if let Some(id) = target_operation_id {
            if op.id != id {
                continue;
            }
        }

        let op_result = dry_run_operation(op);
        if !op_result.ok {
            batch_ok = false;
        }
        results.push(op_result);
    }

    if results.is_empty() && target_operation_id.is_some() {
        batch_ok = false;
        batch_errors.push(format!(
            "Operation ID not found: {}",
            target_operation_id.unwrap()
        ));
    }

    Ok(EditorBatchResult {
        ok: batch_ok,
        results,
        safety: SafetyResult {
            ok: batch_ok,
            errors: batch_errors,
            warnings: Vec::new(),
            notes: Vec::new(),
        },
    })
}

/// Returns a structured `DryRunReport` (used by the `dry-run` command).
///
/// When `workspace` is provided the workspace directory is scanned and each
/// allowed operation's target file is checked for existence. Missing targets
/// are reported and set `safe_to_apply` to `false`.
///
/// Does not write to disk. Never modifies real files or RPF archives.
pub fn build_dry_run_report(
    patch_plan_path: &Path,
    workspace: Option<&Path>,
) -> Result<DryRunReport> {
    let content = fs::read_to_string(patch_plan_path)
        .with_context(|| format!("Failed to read patch plan: {}", patch_plan_path.display()))?;

    let plan: EditorPlan =
        serde_json::from_str(&content).with_context(|| "Failed to parse patch plan JSON")?;

    let mut targets: Vec<DryRunTarget> = Vec::new();
    let mut blocked: Vec<DryRunBlockedItem> = Vec::new();
    let mut warnings: Vec<DryRunWarning> = Vec::new();
    let mut blocked_op_ids: HashSet<String> = HashSet::new();

    for op in &plan.operations {
        let eval = evaluate_operation(op);

        if eval.ok {
            targets.push(DryRunTarget {
                operation_id: eval.operation_id.clone(),
                file_path: eval.file_path.clone(),
                would_change: eval.would_change,
                validators_planned: eval.validators_planned,
            });
        } else {
            blocked_op_ids.insert(eval.operation_id.clone());
            for item in &eval.block_items {
                blocked.push(DryRunBlockedItem {
                    operation_id: eval.operation_id.clone(),
                    file_path: eval.file_path.clone(),
                    reason: item.reason.clone(),
                    block_type: item.block_type.to_string(),
                });
            }
        }

        for msg in &eval.warnings {
            warnings.push(DryRunWarning {
                operation_id: eval.operation_id.clone(),
                message: msg.clone(),
            });
        }
    }

    // Workspace existence check (read-only)
    let mut missing_targets: Vec<DryRunMissingTarget> = Vec::new();
    if let Some(ws_path) = workspace {
        let inventory = crate::inventory::scanner::scan_workspace(ws_path)?;
        let allowed_paths: Vec<String> = targets.iter().map(|t| t.file_path.clone()).collect();
        for m in crate::inventory::scanner::check_targets(&inventory, &allowed_paths) {
            missing_targets.push(DryRunMissingTarget {
                target_path: m.target_path,
                reason: m.reason,
            });
        }
    }

    let total_operations = plan.operations.len();
    let allowed_operations = targets.len();
    let blocked_operations = blocked_op_ids.len();
    let warning_count = warnings.len();
    let missing_target_count = missing_targets.len();
    let safe_to_apply = blocked.is_empty() && missing_targets.is_empty();

    let status = if !safe_to_apply {
        DryRunStatus::Blocked
    } else if !warnings.is_empty() {
        DryRunStatus::HasWarnings
    } else {
        DryRunStatus::AllClear
    };

    Ok(DryRunReport {
        safe_to_apply,
        status,
        targets,
        blocked,
        warnings,
        missing_targets,
        summary: DryRunSummary {
            total_operations,
            allowed_operations,
            blocked_operations,
            warning_count,
            missing_target_count,
        },
    })
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Converts an `OperationEvaluation` to the legacy string-error result type.
fn dry_run_operation(op: &EditorOperation) -> EditorOperationResult {
    let eval = evaluate_operation(op);
    let errors: Vec<String> = eval.block_items.iter().map(|b| b.reason.clone()).collect();
    EditorOperationResult {
        ok: eval.ok,
        mode: "dry_run".to_string(),
        operation_id: eval.operation_id,
        file_path: eval.file_path,
        would_change: eval.would_change,
        would_create_backup: true,
        validators_planned: eval.validators_planned,
        errors,
        warnings: eval.warnings,
        notes: Vec::new(),
        summary: serde_json::json!({}),
    }
}

fn is_binary(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "ytd" | "ypt" | "ysc" | "gfx" | "fxc" | "pso" | "dll" | "exe"
    )
}

fn is_rpf(path: &str) -> bool {
    path.to_lowercase().ends_with(".rpf")
}

fn is_unrelated_component(path: &str) -> bool {
    let p = path.to_lowercase();
    p.contains("tracer")
        || p.contains("hit_effect")
        || p.contains("kill_effect")
        || p.contains("minimap_hud")
}
