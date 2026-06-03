use super::model::*;
use crate::editors::editor_contract::{EditorOperation, EditorPlan};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

// ── Scope constants (mirrors evaluate_operation in dry_run.rs) ────────────────

const ALLOWED_FIRST_PATCH: &[&str] = &[
    "visualsettings.dat",
    "cloudkeyframes.xml",
    "timecycle_mods_1.xml",
];

/// Operation types the text apply engine actually supports.
pub const SUPPORTED_APPLY_OP_TYPES: &[&str] = &["text_replace", "text_append", "text_prepend"];

// ── Private helpers ───────────────────────────────────────────────────────────

fn file_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn is_binary_ext(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "ytd" | "ypt" | "ysc" | "gfx" | "fxc" | "pso" | "dll" | "exe" | "rpf"
    )
}

fn has_binary_content(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

/// Scope/phase/path-safety check for one operation (apply-specific, self-contained).
/// Returns `Some(blocked_item)` if blocked, `None` if ok.
fn check_scope(op: &EditorOperation) -> Option<ApplyBlockedItem> {
    if op.phase != "first_patch" {
        return Some(ApplyBlockedItem {
            operation_id: op.id.clone(),
            file_path: op.path.clone(),
            reason: format!(
                "Operation phase '{}' not supported in first_controlled_patch mode",
                op.phase
            ),
            block_type: "invalid_phase".to_string(),
        });
    }

    // Reject absolute paths and traversal segments.
    if op.path.starts_with('/')
        || op.path.starts_with('\\')
        || op.path.split('/').any(|seg| seg == "..")
        || op.path.split('\\').any(|seg| seg == "..")
    {
        return Some(ApplyBlockedItem {
            operation_id: op.id.clone(),
            file_path: op.path.clone(),
            reason: format!("Unsafe path rejected: {}", op.path),
            block_type: "unsafe_path".to_string(),
        });
    }

    let file_name = file_basename(&op.path);
    if !ALLOWED_FIRST_PATCH.contains(&file_name.as_str()) {
        return Some(ApplyBlockedItem {
            operation_id: op.id.clone(),
            file_path: op.path.clone(),
            reason: format!("File not in allowed first_patch scope: {}", op.path),
            block_type: "not_in_scope".to_string(),
        });
    }

    if is_binary_ext(&op.path) || op.path.to_lowercase().ends_with(".rpf") {
        return Some(ApplyBlockedItem {
            operation_id: op.id.clone(),
            file_path: op.path.clone(),
            reason: format!("Binary or RPF file cannot be text-patched: {}", op.path),
            block_type: "binary_file".to_string(),
        });
    }

    None
}

/// Apply a single text operation to in-memory content.
/// Returns `(new_content, changed)`. Caller guarantees pre-validation passed.
fn apply_single_op(content: &str, op: &EditorOperation) -> (String, bool) {
    let vt = match op.value_target.as_ref() {
        Some(v) => v,
        None => return (content.to_string(), false),
    };

    match op.op_type.as_str() {
        "text_replace" => {
            let search = vt.get("search").and_then(|v| v.as_str()).unwrap_or("");
            let replacement = vt.get("replace").and_then(|v| v.as_str()).unwrap_or("");
            let new = content.replace(search, replacement);
            let changed = new != content;
            (new, changed)
        }
        "text_append" => {
            let text = vt.as_str().unwrap_or("");
            if text.is_empty() {
                return (content.to_string(), false);
            }
            let sep = if content.ends_with('\n') { "" } else { "\n" };
            (format!("{}{}{}", content, sep, text), true)
        }
        "text_prepend" => {
            let text = vt.as_str().unwrap_or("");
            if text.is_empty() {
                return (content.to_string(), false);
            }
            (format!("{}{}", text, content), true)
        }
        _ => (content.to_string(), false),
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Apply supported PatchPlan text operations to files already in `stage_dir`.
///
/// Safety guarantees:
/// - Only files inside `stage_dir` are written to.
/// - If *any* operation fails pre-validation, no files are modified at all.
/// - On write failure during the apply phase, all previously written files are
///   restored to their original content (global rollback).
/// - The source workspace is never read or written.
pub fn apply_patch_plan_to_stage(plan_path: &Path, stage_dir: &Path) -> Result<ApplyReport> {
    let raw = fs::read_to_string(plan_path)
        .with_context(|| format!("Failed to read patch plan: {}", plan_path.display()))?;
    let plan: EditorPlan =
        serde_json::from_str(&raw).with_context(|| "Failed to parse patch plan JSON")?;

    let total_operations = plan.operations.len();
    let mut pre_blocked: Vec<ApplyBlockedItem> = Vec::new();
    let mut unsupported_count = 0usize;

    let stage_canon = stage_dir
        .canonicalize()
        .unwrap_or_else(|_| stage_dir.to_path_buf());

    // ── Pre-validation pass (no writes) ──────────────────────────────────────
    for op in &plan.operations {
        // 1. Scope / phase / path-safety
        if let Some(block) = check_scope(op) {
            pre_blocked.push(block);
            continue;
        }

        // 2. Resolve target inside stage_dir
        let rel = &op.path;
        let target_abs = stage_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));

        // 3. File existence check
        if !target_abs.exists() {
            pre_blocked.push(ApplyBlockedItem {
                operation_id: op.id.clone(),
                file_path: rel.clone(),
                reason: format!("Target file not found in stage directory: {}", rel),
                block_type: "missing_staged_file".to_string(),
            });
            continue;
        }

        // 4. Canonical containment guard (catches symlink escapes)
        if let Ok(canon) = target_abs.canonicalize() {
            if !canon.starts_with(&stage_canon) {
                pre_blocked.push(ApplyBlockedItem {
                    operation_id: op.id.clone(),
                    file_path: rel.clone(),
                    reason: format!("Path escapes stage directory: {}", rel),
                    block_type: "path_traversal".to_string(),
                });
                continue;
            }
        }

        // 5. Read bytes; binary content check then UTF-8 check
        let file_bytes = fs::read(&target_abs)
            .with_context(|| format!("Failed to read staged file: {}", target_abs.display()))?;

        if has_binary_content(&file_bytes) {
            pre_blocked.push(ApplyBlockedItem {
                operation_id: op.id.clone(),
                file_path: rel.clone(),
                reason: format!(
                    "Staged file appears to be binary (null bytes detected): {}",
                    rel
                ),
                block_type: "binary_file".to_string(),
            });
            continue;
        }

        let file_text = match std::str::from_utf8(&file_bytes) {
            Ok(t) => t.to_string(),
            Err(_) => {
                pre_blocked.push(ApplyBlockedItem {
                    operation_id: op.id.clone(),
                    file_path: rel.clone(),
                    reason: format!("Staged file is not valid UTF-8: {}", rel),
                    block_type: "non_utf8_text".to_string(),
                });
                continue;
            }
        };

        // 6. Supported apply op type check
        if !SUPPORTED_APPLY_OP_TYPES.contains(&op.op_type.as_str()) {
            unsupported_count += 1;
            pre_blocked.push(ApplyBlockedItem {
                operation_id: op.id.clone(),
                file_path: rel.clone(),
                reason: format!(
                    "Operation type '{}' is not supported by the text apply engine (supported: {})",
                    op.op_type,
                    SUPPORTED_APPLY_OP_TYPES.join(", ")
                ),
                block_type: "unsupported_op_type".to_string(),
            });
            continue;
        }

        // 7. valueTarget presence check
        let vt = match op.value_target.as_ref() {
            Some(v) => v,
            None => {
                pre_blocked.push(ApplyBlockedItem {
                    operation_id: op.id.clone(),
                    file_path: rel.clone(),
                    reason: "Operation is missing 'valueTarget'".to_string(),
                    block_type: "missing_value_target".to_string(),
                });
                continue;
            }
        };

        // 8. text_replace specific: shape + zero-match check
        if op.op_type == "text_replace" {
            let search = match vt.get("search").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => {
                    pre_blocked.push(ApplyBlockedItem {
                        operation_id: op.id.clone(),
                        file_path: rel.clone(),
                        reason:
                            "text_replace requires valueTarget with 'search' and 'replace' string keys"
                                .to_string(),
                        block_type: "invalid_value_target".to_string(),
                    });
                    continue;
                }
            };
            if vt.get("replace").and_then(|v| v.as_str()).is_none() {
                pre_blocked.push(ApplyBlockedItem {
                    operation_id: op.id.clone(),
                    file_path: rel.clone(),
                    reason:
                        "text_replace requires valueTarget with 'search' and 'replace' string keys"
                            .to_string(),
                    block_type: "invalid_value_target".to_string(),
                });
                continue;
            }
            if !file_text.contains(search) {
                pre_blocked.push(ApplyBlockedItem {
                    operation_id: op.id.clone(),
                    file_path: rel.clone(),
                    reason: format!("Search string not found in staged file: {:?}", search),
                    block_type: "search_not_found".to_string(),
                });
                continue;
            }
        }
    }

    // If any operation is blocked, return immediately without touching any file.
    if !pre_blocked.is_empty() {
        let blocked_count = pre_blocked.len();
        return Ok(ApplyReport {
            safe_applied: false,
            status: ApplyStatus::Blocked,
            file_results: Vec::new(),
            blocked: pre_blocked,
            summary: ApplySummary {
                total_operations,
                applied_count: 0,
                blocked_count,
                unsupported_count,
            },
        });
    }

    // ── Apply phase ───────────────────────────────────────────────────────────

    // Collect unique target paths in operation order.
    let mut target_paths: Vec<String> = Vec::new();
    {
        let mut seen: HashSet<String> = HashSet::new();
        for op in &plan.operations {
            if seen.insert(op.path.clone()) {
                target_paths.push(op.path.clone());
            }
        }
    }

    // Read original content for global rollback.
    let mut original_bytes: HashMap<String, Vec<u8>> = HashMap::new();
    let mut current_contents: HashMap<String, String> = HashMap::new();
    for path in &target_paths {
        let abs = stage_dir.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = fs::read(&abs)
            .with_context(|| format!("Failed to read original content of: {}", abs.display()))?;
        let text = String::from_utf8(bytes.clone())
            .with_context(|| format!("Non-UTF-8 content in apply phase: {}", path))?;
        original_bytes.insert(path.clone(), bytes);
        current_contents.insert(path.clone(), text);
    }

    // Build per-file op-result trackers (same order as target_paths).
    // Each entry: (path, op_results, modified)
    let mut file_op_data: Vec<(String, Vec<ApplyOperationResult>, bool)> = target_paths
        .iter()
        .map(|p| (p.clone(), Vec::new(), false))
        .collect();

    // Build a path → index map for fast lookup.
    let path_index: HashMap<String, usize> = target_paths
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), i))
        .collect();

    // Apply each operation in order against the in-memory current_contents.
    for op in &plan.operations {
        let current = current_contents
            .get(&op.path)
            .map(String::as_str)
            .unwrap_or("");
        let lines_before = current.lines().count();
        let (new_content, changed) = apply_single_op(current, op);
        let lines_after = new_content.lines().count();

        if let Some(&idx) = path_index.get(&op.path) {
            let entry = &mut file_op_data[idx];
            entry.1.push(ApplyOperationResult {
                operation_id: op.id.clone(),
                file_path: op.path.clone(),
                op_type: op.op_type.clone(),
                status: "applied".to_string(),
                changed,
                reason: None,
                lines_before: Some(lines_before),
                lines_after: Some(lines_after),
            });
            entry.2 = entry.2 || changed;
        }

        *current_contents.get_mut(&op.path).unwrap() = new_content;
    }

    // Write all modified files; on any failure restore all written files.
    let mut successfully_written: Vec<String> = Vec::new();
    let mut write_error: Option<String> = None;

    for path in &target_paths {
        let abs = stage_dir.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let new_content = current_contents.get(path).unwrap();
        if let Err(e) = fs::write(&abs, new_content.as_bytes()) {
            write_error = Some(format!("Failed to write {}: {}", abs.display(), e));
            break;
        }
        successfully_written.push(path.clone());
    }

    if let Some(err) = write_error {
        // Global rollback: restore every file that was successfully written.
        for path in &successfully_written {
            let abs = stage_dir.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(orig) = original_bytes.get(path) {
                let _ = fs::write(&abs, orig);
            }
        }

        let rolled_back_results: Vec<ApplyFileResult> = file_op_data
            .into_iter()
            .map(|(path, mut ops, modified)| {
                for op_result in &mut ops {
                    op_result.status = "rolled_back".to_string();
                }
                ApplyFileResult {
                    file_path: path,
                    modified,
                    operations: ops,
                    rolled_back: true,
                }
            })
            .collect();

        return Ok(ApplyReport {
            safe_applied: false,
            status: ApplyStatus::Blocked,
            file_results: rolled_back_results,
            blocked: vec![ApplyBlockedItem {
                operation_id: "apply_error".to_string(),
                file_path: String::new(),
                reason: err,
                block_type: "apply_error".to_string(),
            }],
            summary: ApplySummary {
                total_operations,
                applied_count: 0,
                blocked_count: 1,
                unsupported_count: 0,
            },
        });
    }

    let applied_count: usize = file_op_data.iter().map(|(_, ops, _)| ops.len()).sum();

    let file_results: Vec<ApplyFileResult> = file_op_data
        .into_iter()
        .map(|(path, ops, modified)| ApplyFileResult {
            file_path: path,
            modified,
            operations: ops,
            rolled_back: false,
        })
        .collect();

    Ok(ApplyReport {
        safe_applied: true,
        status: ApplyStatus::AllApplied,
        file_results,
        blocked: Vec::new(),
        summary: ApplySummary {
            total_operations,
            applied_count,
            blocked_count: 0,
            unsupported_count: 0,
        },
    })
}
