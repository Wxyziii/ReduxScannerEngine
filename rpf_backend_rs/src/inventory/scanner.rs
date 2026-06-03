use super::model::*;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const BINARY_EXTENSIONS: &[&str] = &[
    "ytd", "ypt", "ysc", "gfx", "fxc", "pso", "dll", "exe", "rpf",
];
const HASH_MAX_BYTES: u64 = 1024 * 1024;

/// Normalize a path to use forward slashes with no leading or trailing slash.
/// Case is preserved (no lowercasing) for cross-platform correctness.
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

fn file_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn is_text_like(ext: &str) -> bool {
    !BINARY_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Recursively scan `workspace` and return an `InventoryReport` of all files found.
///
/// This function is read-only and never modifies any file.
pub fn scan_workspace(workspace: &Path) -> Result<InventoryReport> {
    let workspace_str = normalize_path(&workspace.to_string_lossy());
    let mut files: Vec<InventoryFile> = Vec::new();
    scan_dir(workspace, workspace, &mut files)?;

    let status = if files.is_empty() {
        InventoryScanStatus::Empty
    } else {
        InventoryScanStatus::Ok
    };

    let total_files = files.len();
    let text_like_files = files.iter().filter(|f| f.is_text_like).count();
    let binary_like_files = total_files - text_like_files;

    let mut ext_set: BTreeSet<String> = BTreeSet::new();
    for f in &files {
        if !f.extension.is_empty() {
            ext_set.insert(f.extension.clone());
        }
    }

    Ok(InventoryReport {
        status,
        workspace_path: workspace_str,
        files,
        missing_targets: Vec::new(),
        summary: InventorySummary {
            total_files,
            text_like_files,
            binary_like_files,
            extensions: ext_set.into_iter().collect(),
        },
    })
}

fn scan_dir(root: &Path, current: &Path, files: &mut Vec<InventoryFile>) -> Result<()> {
    for entry in fs::read_dir(current)
        .with_context(|| format!("Failed to read directory: {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::metadata(&path)?;

        if meta.is_dir() {
            scan_dir(root, &path, files)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .map(|p| normalize_path(&p.to_string_lossy()))
                .unwrap_or_else(|_| normalize_path(&path.to_string_lossy()));

            let ext = file_extension(&rel);
            let is_text = is_text_like(&ext);
            let size = meta.len();

            let sha256 = if size <= HASH_MAX_BYTES {
                let data = fs::read(&path)?;
                let mut hasher = Sha256::new();
                hasher.update(&data);
                Some(hex::encode(hasher.finalize()))
            } else {
                None
            };

            files.push(InventoryFile {
                path: rel,
                extension: ext,
                size_bytes: size,
                is_text_like: is_text,
                sha256,
            });
        }
    }
    Ok(())
}

/// Check which targets from a PatchPlan are absent in the inventory.
///
/// Paths are normalized before comparison (backslashes → forward slashes).
/// Matching is exact (no suffix fallback) to prevent false positives.
pub fn check_targets(
    inventory: &InventoryReport,
    targets: &[String],
) -> Vec<InventoryMissingTarget> {
    let mut missing = Vec::new();
    for target in targets {
        let normalized = normalize_path(target);
        let found = inventory.files.iter().any(|f| f.path == normalized);
        if !found {
            missing.push(InventoryMissingTarget {
                target_path: normalized,
                reason: "File not found in workspace inventory".to_string(),
            });
        }
    }
    missing
}
