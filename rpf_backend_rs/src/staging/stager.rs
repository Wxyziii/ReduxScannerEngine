use super::model::*;
use crate::editors::dry_run::build_dry_run_report;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const HASH_MAX_BYTES: u64 = 1024 * 1024;

/// Copy validated PatchPlan target files from `workspace_path` into `stage_dir`.
///
/// Safety guarantees:
/// - The source `workspace_path` is never written to.
/// - No RPF archives are created or modified.
/// - If the dry-run is blocked (scope violation or missing targets), nothing is copied.
/// - If a copy fails mid-way, all already-copied files are removed before returning an error.
/// - `stage_dir` must be different from `workspace_path`.
pub fn stage_patch_plan(
    plan_path: &Path,
    workspace_path: &Path,
    stage_dir: &Path,
) -> Result<StageReport> {
    // Guard: stage_dir must not be the same as workspace_path
    let ws_canon = workspace_path.canonicalize().with_context(|| {
        format!(
            "Cannot canonicalize workspace: {}",
            workspace_path.display()
        )
    })?;
    // stage_dir may not exist yet — create it first, then canonicalize
    fs::create_dir_all(stage_dir)
        .with_context(|| format!("Cannot create stage_dir: {}", stage_dir.display()))?;
    let stage_canon = stage_dir
        .canonicalize()
        .with_context(|| format!("Cannot canonicalize stage_dir: {}", stage_dir.display()))?;

    if ws_canon == stage_canon {
        anyhow::bail!(
            "stage_dir and workspace_path must be different paths (both resolve to {})",
            ws_canon.display()
        );
    }

    // Run dry-run with workspace validation (read-only)
    let dry_run = build_dry_run_report(plan_path, Some(workspace_path))?;

    if !dry_run.safe_to_apply {
        // Collect all reasons into StageBlockedItems
        let mut blocked: Vec<StageBlockedItem> = dry_run
            .blocked
            .iter()
            .map(|b| StageBlockedItem {
                path: b.file_path.clone(),
                reason: b.reason.clone(),
                block_type: b.block_type.clone(),
            })
            .collect();
        for m in &dry_run.missing_targets {
            blocked.push(StageBlockedItem {
                path: m.target_path.clone(),
                reason: m.reason.clone(),
                block_type: "missing_target".to_string(),
            });
        }
        let total_targets = dry_run.summary.total_operations;
        let blocked_count = blocked.len();
        return Ok(StageReport {
            safe_to_stage: false,
            status: StageStatus::Blocked,
            files: Vec::new(),
            blocked,
            summary: StageSummary {
                total_targets,
                staged_count: 0,
                blocked_count,
            },
            manifest_path: None,
        });
    }

    // Stage each allowed target file
    let mut staged_files: Vec<StageFile> = Vec::new();
    let mut staged_paths_for_rollback: Vec<std::path::PathBuf> = Vec::new();

    let result = (|| -> Result<()> {
        for target in &dry_run.targets {
            let rel = &target.file_path;
            let src_path = workspace_path.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let dst_path = stage_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));

            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Cannot create parent dirs for {}", dst_path.display())
                })?;
            }

            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "Failed to copy {} -> {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
            staged_paths_for_rollback.push(dst_path.clone());

            let size = fs::metadata(&dst_path)?.len();
            let sha256 = if size <= HASH_MAX_BYTES {
                let data = fs::read(&dst_path)?;
                let mut hasher = Sha256::new();
                hasher.update(&data);
                Some(hex::encode(hasher.finalize()))
            } else {
                None
            };
            let extension = Path::new(rel)
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            staged_files.push(StageFile {
                source_path: rel.clone(),
                staged_path: rel.clone(),
                source_abs: src_path.to_string_lossy().to_string(),
                staged_abs: dst_path.to_string_lossy().to_string(),
                size_bytes: size,
                extension,
                sha256,
                status: "staged".to_string(),
            });
        }
        Ok(())
    })();

    if let Err(e) = result {
        // Rollback: remove any files we already copied
        for p in &staged_paths_for_rollback {
            let _ = fs::remove_file(p);
        }
        return Err(e);
    }

    // Write stage manifest
    let manifest_path = stage_dir.join("stage_manifest.json");
    let manifest = StageManifest {
        plan_path: plan_path.to_string_lossy().to_string(),
        workspace_path: workspace_path.to_string_lossy().to_string(),
        stage_dir: stage_dir.to_string_lossy().to_string(),
        safe_to_stage: true,
        files: staged_files.clone(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, manifest_json).with_context(|| {
        format!(
            "Failed to write stage manifest: {}",
            manifest_path.display()
        )
    })?;

    let staged_count = staged_files.len();
    let total_targets = dry_run.summary.total_operations;

    Ok(StageReport {
        safe_to_stage: true,
        status: StageStatus::AllClear,
        files: staged_files,
        blocked: Vec::new(),
        summary: StageSummary {
            total_targets,
            staged_count,
            blocked_count: 0,
        },
        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
    })
}
