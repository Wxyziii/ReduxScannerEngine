use super::model::*;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Hard limit: files larger than this many lines are blocked as diff_too_large.
const MAX_DIFF_LINES: usize = 5000;
/// Lines of context shown around each change.
const CONTEXT_SIZE: usize = 3;
/// Maximum changed lines (add + remove) included in the hunk preview per file.
const MAX_PREVIEW_CHANGED_LINES: usize = 20;

// ── Private helpers ───────────────────────────────────────────────────────────

fn is_binary_content(bytes: &[u8]) -> bool {
    bytes.contains(&0)
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

/// Recursively collect files under `current_dir`, stripping `base_dir` for
/// relative paths. Skips `stage_manifest.json` (internal staging metadata).
fn collect_stage_files(base_dir: &Path, current_dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(current_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_stage_files(base_dir, &path, out);
        } else if path.is_file() {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            // Skip internal staging metadata.
            if name == "stage_manifest.json" {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(base_dir) {
                let normalized = rel
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches('/')
                    .to_string();
                out.push((path, normalized));
            }
        }
    }
}

// ── LCS diff ─────────────────────────────────────────────────────────────────

struct RawEntry {
    lt: DiffLineType,
    old_no: Option<usize>,
    new_no: Option<usize>,
    content: String,
}

enum BtOp {
    Keep(String),
    Delete(String),
    Insert(String),
}

/// LCS-based line diff. Returns ordered `(type, old_lineno, new_lineno, content)` entries.
///
/// Tie-break: prefer Delete over Insert for deterministic output.
/// Memory: O(n*m) DP table — callers must guard with MAX_DIFF_LINES.
fn lcs_diff(old_lines: &[&str], new_lines: &[&str]) -> Vec<RawEntry> {
    let n = old_lines.len();
    let m = new_lines.len();

    // Build DP table (u32 to halve memory vs usize on 64-bit).
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = if old_lines[i - 1] == new_lines[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    // Backtrace in reverse, preferring Delete over Insert.
    let mut ops: Vec<BtOp> = Vec::with_capacity(n + m);
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            ops.push(BtOp::Keep(old_lines[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if i > 0 && (j == 0 || dp[i - 1][j] >= dp[i][j - 1]) {
            ops.push(BtOp::Delete(old_lines[i - 1].to_string()));
            i -= 1;
        } else {
            ops.push(BtOp::Insert(new_lines[j - 1].to_string()));
            j -= 1;
        }
    }
    ops.reverse();

    // Assign 1-based line numbers as we replay the edit script.
    let mut result = Vec::with_capacity(ops.len());
    let mut old_no = 0usize;
    let mut new_no = 0usize;
    for op in ops {
        match op {
            BtOp::Keep(content) => {
                old_no += 1;
                new_no += 1;
                result.push(RawEntry {
                    lt: DiffLineType::Context,
                    old_no: Some(old_no),
                    new_no: Some(new_no),
                    content,
                });
            }
            BtOp::Delete(content) => {
                old_no += 1;
                result.push(RawEntry {
                    lt: DiffLineType::Remove,
                    old_no: Some(old_no),
                    new_no: None,
                    content,
                });
            }
            BtOp::Insert(content) => {
                new_no += 1;
                result.push(RawEntry {
                    lt: DiffLineType::Add,
                    old_no: None,
                    new_no: Some(new_no),
                    content,
                });
            }
        }
    }
    result
}

// ── Hunk generation ───────────────────────────────────────────────────────────

fn build_hunks(raw: &[RawEntry]) -> (Vec<DiffHunk>, bool, usize, usize) {
    let n = raw.len();
    let lines_added = raw.iter().filter(|e| e.lt == DiffLineType::Add).count();
    let lines_removed = raw.iter().filter(|e| e.lt == DiffLineType::Remove).count();

    // Mark which positions are within CONTEXT_SIZE of a change.
    let is_changed: Vec<bool> = raw.iter().map(|e| e.lt != DiffLineType::Context).collect();
    let mut in_zone = vec![false; n];
    for i in 0..n {
        if is_changed[i] {
            let start = i.saturating_sub(CONTEXT_SIZE);
            let end = (i + CONTEXT_SIZE + 1).min(n);
            for k in start..end {
                in_zone[k] = true;
            }
        }
    }

    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut changed_budget = MAX_PREVIEW_CHANGED_LINES;
    let mut preview_truncated = false;
    let mut i = 0;

    while i < n {
        if !in_zone[i] || changed_budget == 0 {
            if in_zone[i] && changed_budget == 0 {
                preview_truncated = true;
            }
            i += 1;
            continue;
        }

        // Consume this hunk run.
        let hunk_start = i;
        while i < n && in_zone[i] {
            i += 1;
        }
        let hunk_end = i;

        let mut hunk_lines: Vec<DiffLine> = Vec::new();
        for k in hunk_start..hunk_end {
            let e = &raw[k];
            if e.lt != DiffLineType::Context {
                if changed_budget == 0 {
                    preview_truncated = true;
                    break;
                }
                changed_budget -= 1;
            }
            hunk_lines.push(DiffLine {
                line_type: e.lt.clone(),
                old_line_number: e.old_no,
                new_line_number: e.new_no,
                content: e.content.clone(),
            });
        }

        if !hunk_lines.is_empty() {
            // Derive hunk range header from first available line numbers.
            let old_start = hunk_lines
                .iter()
                .filter_map(|l| l.old_line_number)
                .next()
                .unwrap_or(0);
            let new_start = hunk_lines
                .iter()
                .filter_map(|l| l.new_line_number)
                .next()
                .unwrap_or(0);
            let old_count = hunk_lines
                .iter()
                .filter(|l| matches!(l.line_type, DiffLineType::Context | DiffLineType::Remove))
                .count();
            let new_count = hunk_lines
                .iter()
                .filter(|l| matches!(l.line_type, DiffLineType::Context | DiffLineType::Add))
                .count();
            let label = format!(
                "@@ -{},{} +{},{} @@",
                old_start, old_count, new_start, new_count
            );
            hunks.push(DiffHunk {
                context_label: label,
                lines: hunk_lines,
            });
        }
    }

    // If total changed exceeds budget, mark truncated regardless.
    if lines_added + lines_removed > MAX_PREVIEW_CHANGED_LINES {
        preview_truncated = true;
    }

    (hunks, preview_truncated, lines_added, lines_removed)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Compare each staged file against its workspace counterpart and return a
/// structured diff report.
///
/// Safety guarantees:
/// - Neither the workspace nor the stage directory is modified.
/// - `stage_manifest.json` is silently skipped (internal staging metadata).
/// - Files exceeding MAX_DIFF_LINES are blocked as `diff_too_large`.
pub fn build_stage_diff_report(workspace_path: &Path, stage_dir: &Path) -> Result<DiffReport> {
    // Collect all staged files (recursively, skipping stage_manifest.json).
    let mut staged: Vec<(PathBuf, String)> = Vec::new();
    collect_stage_files(stage_dir, stage_dir, &mut staged);
    // Sort for deterministic output order.
    staged.sort_by(|a, b| a.1.cmp(&b.1));

    let mut files: Vec<DiffFile> = Vec::new();
    let mut blocked: Vec<DiffBlockedItem> = Vec::new();

    for (staged_abs, rel_path) in &staged {
        let orig_abs = workspace_path.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));

        // 1. Workspace counterpart must exist.
        if !orig_abs.exists() {
            blocked.push(DiffBlockedItem {
                path: rel_path.clone(),
                reason: format!("File not found in workspace: {}", orig_abs.display()),
                block_type: "missing_original".to_string(),
            });
            continue;
        }

        // 2. Binary extension check (staged).
        if is_binary_ext(rel_path) {
            blocked.push(DiffBlockedItem {
                path: rel_path.clone(),
                reason: format!("Binary extension, cannot produce text diff: {}", rel_path),
                block_type: "binary_file".to_string(),
            });
            continue;
        }

        // 3. Read staged bytes → binary content check → UTF-8 decode.
        let staged_bytes = fs::read(staged_abs)
            .with_context(|| format!("Failed to read staged file: {}", staged_abs.display()))?;
        if is_binary_content(&staged_bytes) {
            blocked.push(DiffBlockedItem {
                path: rel_path.clone(),
                reason: format!("Staged file has binary content (null bytes): {}", rel_path),
                block_type: "binary_file".to_string(),
            });
            continue;
        }
        let staged_text = match std::str::from_utf8(&staged_bytes) {
            Ok(t) => t.to_string(),
            Err(_) => {
                blocked.push(DiffBlockedItem {
                    path: rel_path.clone(),
                    reason: format!("Staged file is not valid UTF-8: {}", rel_path),
                    block_type: "non_utf8_file".to_string(),
                });
                continue;
            }
        };

        // 4. Read workspace bytes → same checks.
        let orig_bytes = fs::read(&orig_abs)
            .with_context(|| format!("Failed to read workspace file: {}", orig_abs.display()))?;
        if is_binary_content(&orig_bytes) {
            blocked.push(DiffBlockedItem {
                path: rel_path.clone(),
                reason: format!(
                    "Workspace file has binary content (null bytes): {}",
                    rel_path
                ),
                block_type: "binary_file".to_string(),
            });
            continue;
        }
        let orig_text = match std::str::from_utf8(&orig_bytes) {
            Ok(t) => t.to_string(),
            Err(_) => {
                blocked.push(DiffBlockedItem {
                    path: rel_path.clone(),
                    reason: format!("Workspace file is not valid UTF-8: {}", rel_path),
                    block_type: "non_utf8_file".to_string(),
                });
                continue;
            }
        };

        // 5. Split into lines. `changed` is based on line content comparison
        //    (line-ending agnostic).
        let orig_lines_vec: Vec<&str> = orig_text.lines().collect();
        let staged_lines_vec: Vec<&str> = staged_text.lines().collect();
        let changed = orig_lines_vec != staged_lines_vec;

        let orig_line_count = orig_lines_vec.len();
        let staged_line_count = staged_lines_vec.len();

        // 6. Guard against huge files.
        if orig_line_count > MAX_DIFF_LINES || staged_line_count > MAX_DIFF_LINES {
            blocked.push(DiffBlockedItem {
                path: rel_path.clone(),
                reason: format!(
                    "File too large to diff ({} / {} lines, limit {})",
                    orig_line_count, staged_line_count, MAX_DIFF_LINES
                ),
                block_type: "diff_too_large".to_string(),
            });
            continue;
        }

        // 7. If unchanged, emit a simple DiffFile with no hunks.
        if !changed {
            files.push(DiffFile {
                relative_path: rel_path.clone(),
                orig_abs: orig_abs.to_string_lossy().into_owned(),
                staged_abs: staged_abs.to_string_lossy().into_owned(),
                changed: false,
                orig_line_count,
                staged_line_count,
                lines_added: 0,
                lines_removed: 0,
                hunks: Vec::new(),
                preview_truncated: false,
                status: DiffStatus::Unchanged,
            });
            continue;
        }

        // 8. Compute LCS diff and build hunks.
        let raw = lcs_diff(&orig_lines_vec, &staged_lines_vec);
        let (hunks, preview_truncated, lines_added, lines_removed) = build_hunks(&raw);

        files.push(DiffFile {
            relative_path: rel_path.clone(),
            orig_abs: orig_abs.to_string_lossy().into_owned(),
            staged_abs: staged_abs.to_string_lossy().into_owned(),
            changed: true,
            orig_line_count,
            staged_line_count,
            lines_added,
            lines_removed,
            hunks,
            preview_truncated,
            status: DiffStatus::Changed,
        });
    }

    let changed_count = files.iter().filter(|f| f.changed).count();
    let unchanged_count = files.iter().filter(|f| !f.changed).count();
    let blocked_count = blocked.len();
    let diffed_clean = blocked_count == 0;

    Ok(DiffReport {
        diffed_clean,
        files,
        blocked,
        summary: DiffSummary {
            total_files: staged.len(),
            changed_count,
            unchanged_count,
            blocked_count,
        },
    })
}
