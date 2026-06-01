use super::model::*;
use crate::diff::preview::build_stage_diff_report;
use crate::editors::dry_run::build_dry_run_report;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

// ── Constants ─────────────────────────────────────────────────────────────────

const BUNDLE_FORMAT: &str = "redux_patch_bundle";
const BUNDLE_VERSION: &str = "1";
/// Files larger than this are recorded without a SHA-256 hash.
const HASH_MAX_BYTES: u64 = 1024 * 1024;

/// Internal metadata / report files that may live in a stage dir but must never
/// be copied into `files/` as if they were patched game assets. They are bundled
/// separately by name (or regenerated) at the bundle root.
const STAGE_METADATA_NAMES: &[&str] = &[
    "stage_manifest.json",
    "apply_report.json",
    "diff_report.json",
    "bundle_manifest.json",
    "export_report.json",
];

// ── Private helpers ───────────────────────────────────────────────────────────

/// Recursively collect files under `current_dir`, normalizing relative paths
/// against `base_dir`. Skips internal staging/report metadata (see
/// [`STAGE_METADATA_NAMES`]) so report JSONs are never copied as game assets.
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
            if STAGE_METADATA_NAMES.contains(&name.as_str()) {
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

/// Canonicalize a path that may not exist yet by resolving its parent and
/// re-joining the final component. Returns `None` if neither resolves.
fn canonicalize_target(path: &Path) -> Option<PathBuf> {
    if let Ok(c) = path.canonicalize() {
        return Some(c);
    }
    let parent = path.parent()?;
    let file_name = path.file_name()?;
    let parent_canon = parent.canonicalize().ok()?;
    Some(parent_canon.join(file_name))
}

fn extension_of(rel: &str) -> String {
    Path::new(rel)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn hash_file(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Build a blocked report (nothing was written) from a single block item.
fn blocked_report(
    bundle_dir: &Path,
    block_type: &str,
    path: String,
    reason: String,
) -> BundleExportReport {
    BundleExportReport {
        safe_exported: false,
        status: BundleExportStatus::Blocked,
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        manifest_path: None,
        files: Vec::new(),
        included_reports: Vec::new(),
        blocked: vec![BundleBlockedItem {
            path,
            reason,
            block_type: block_type.to_string(),
        }],
        warnings: Vec::new(),
        summary: BundleSummary {
            file_count: 0,
            report_count: 0,
            blocked_count: 1,
            warning_count: 0,
        },
        modifies_rpf: false,
        modifies_source_workspace: false,
        exported_from_stage_only: true,
    }
}

/// Build a blocked report from a list of pre-built block items.
fn blocked_report_many(bundle_dir: &Path, blocked: Vec<BundleBlockedItem>) -> BundleExportReport {
    let blocked_count = blocked.len();
    BundleExportReport {
        safe_exported: false,
        status: BundleExportStatus::Blocked,
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        manifest_path: None,
        files: Vec::new(),
        included_reports: Vec::new(),
        blocked,
        warnings: Vec::new(),
        summary: BundleSummary {
            file_count: 0,
            report_count: 0,
            blocked_count,
            warning_count: 0,
        },
        modifies_rpf: false,
        modifies_source_workspace: false,
        exported_from_stage_only: true,
    }
}

/// Decide whether an existing, non-empty `bundle_dir` is safe to reuse. It is
/// only safe when its single `bundle_manifest.json` reports our bundle format.
fn existing_bundle_is_compatible(bundle_dir: &Path) -> bool {
    let manifest_path = bundle_dir.join("bundle_manifest.json");
    let Ok(content) = fs::read_to_string(&manifest_path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    value
        .get("bundleFormat")
        .and_then(|v| v.as_str())
        .map(|f| f == BUNDLE_FORMAT)
        .unwrap_or(false)
}

fn dir_is_empty(dir: &Path) -> bool {
    match fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Package patched staged files and related reports into a portable bundle.
///
/// Safety guarantees:
/// - The source `workspace_path` is never written to.
/// - The `stage_dir` is only read from — never modified.
/// - No RPF archives are created or modified.
/// - Only `bundle_dir` (a fresh or previously-bundled folder) is written.
pub fn export_patch_bundle(
    plan_path: &Path,
    workspace_path: &Path,
    stage_dir: &Path,
    bundle_dir: &Path,
) -> Result<BundleExportReport, String> {
    // 1. stage_dir must exist and be a directory.
    if !stage_dir.exists() || !stage_dir.is_dir() {
        return Ok(blocked_report(
            bundle_dir,
            "missing_stage_dir",
            stage_dir.to_string_lossy().to_string(),
            format!("Stage directory does not exist: {}", stage_dir.display()),
        ));
    }

    // 2. Collect staged files (skip internal metadata).
    let mut staged: Vec<(PathBuf, String)> = Vec::new();
    collect_stage_files(stage_dir, stage_dir, &mut staged);
    staged.sort_by(|a, b| a.1.cmp(&b.1));

    if staged.is_empty() {
        return Ok(blocked_report(
            bundle_dir,
            "empty_stage_dir",
            stage_dir.to_string_lossy().to_string(),
            format!("No staged files found in: {}", stage_dir.display()),
        ));
    }

    // 3. Path-equality guards (canonicalized).
    let ws_canon = workspace_path.canonicalize().map_err(|e| {
        format!(
            "Cannot canonicalize workspace {}: {}",
            workspace_path.display(),
            e
        )
    })?;
    let stage_canon = stage_dir.canonicalize().map_err(|e| {
        format!(
            "Cannot canonicalize stage_dir {}: {}",
            stage_dir.display(),
            e
        )
    })?;
    let bundle_canon = canonicalize_target(bundle_dir);

    if let Some(bc) = &bundle_canon {
        if bc == &ws_canon {
            return Ok(blocked_report(
                bundle_dir,
                "bundle_dir_equals_workspace",
                bundle_dir.to_string_lossy().to_string(),
                format!("bundle_dir resolves to the workspace: {}", bc.display()),
            ));
        }
        if bc == &stage_canon {
            return Ok(blocked_report(
                bundle_dir,
                "bundle_dir_equals_stage_dir",
                bundle_dir.to_string_lossy().to_string(),
                format!("bundle_dir resolves to the stage_dir: {}", bc.display()),
            ));
        }
    }

    // 4. Validate the PatchPlan against the workspace (read-only dry-run).
    let dry_run = build_dry_run_report(plan_path, Some(workspace_path))
        .map_err(|e| format!("Failed to validate patch plan: {}", e))?;
    if !dry_run.safe_to_apply {
        let mut blocked: Vec<BundleBlockedItem> = dry_run
            .blocked
            .iter()
            .map(|b| BundleBlockedItem {
                path: b.file_path.clone(),
                reason: b.reason.clone(),
                block_type: b.block_type.clone(),
            })
            .collect();
        for m in &dry_run.missing_targets {
            blocked.push(BundleBlockedItem {
                path: m.target_path.clone(),
                reason: m.reason.clone(),
                block_type: "missing_target".to_string(),
            });
        }
        return Ok(blocked_report_many(bundle_dir, blocked));
    }

    // 5. Confirm every required staged target file exists in the stage dir.
    let mut missing_staged: Vec<BundleBlockedItem> = Vec::new();
    for target in &dry_run.targets {
        let rel = &target.file_path;
        let staged_path = stage_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !staged_path.is_file() {
            missing_staged.push(BundleBlockedItem {
                path: rel.clone(),
                reason: format!("Required staged target missing: {}", staged_path.display()),
                block_type: "missing_staged_target".to_string(),
            });
        }
    }
    if !missing_staged.is_empty() {
        return Ok(blocked_report_many(bundle_dir, missing_staged));
    }

    // 6. Non-empty bundle_dir guard — only reuse a folder from a prior safe bundle.
    if bundle_dir.exists()
        && !dir_is_empty(bundle_dir)
        && !existing_bundle_is_compatible(bundle_dir)
    {
        return Ok(blocked_report(
            bundle_dir,
            "bundle_dir_not_empty",
            bundle_dir.to_string_lossy().to_string(),
            format!(
                "bundle_dir is not empty and has no matching bundle_manifest.json: {}",
                bundle_dir.display()
            ),
        ));
    }

    // 7. Generate the diff report (read-only against workspace + stage).
    let diff_report = build_stage_diff_report(workspace_path, stage_dir)
        .map_err(|e| format!("Failed to build diff report: {}", e))?;

    // 8. Create bundle layout and copy staged files into files/.
    let files_root = bundle_dir.join("files");
    fs::create_dir_all(&files_root).map_err(|e| {
        format!(
            "Cannot create bundle files dir {}: {}",
            files_root.display(),
            e
        )
    })?;

    let mut bundle_files: Vec<BundleFile> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for (staged_abs, rel) in &staged {
        let dst = files_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create dir {}: {}", parent.display(), e))?;
        }
        let bytes = fs::read(staged_abs)
            .map_err(|e| format!("Cannot read staged file {}: {}", staged_abs.display(), e))?;
        fs::write(&dst, &bytes)
            .map_err(|e| format!("Cannot write bundle file {}: {}", dst.display(), e))?;

        let size = bytes.len() as u64;
        let sha256 = if size <= HASH_MAX_BYTES {
            Some(hash_file(&bytes))
        } else {
            None
        };
        bundle_files.push(BundleFile {
            relative_path: rel.clone(),
            source_staged_path: staged_abs.to_string_lossy().to_string(),
            exported_path: dst.to_string_lossy().to_string(),
            size_bytes: size,
            extension: extension_of(rel),
            sha256,
        });
    }

    // 9. Copy / generate report files at the bundle root.
    let mut included_reports: Vec<String> = Vec::new();

    // patch_plan.json (copied verbatim from plan_path).
    let plan_bytes = fs::read(plan_path)
        .map_err(|e| format!("Cannot read patch plan {}: {}", plan_path.display(), e))?;
    fs::write(bundle_dir.join("patch_plan.json"), &plan_bytes)
        .map_err(|e| format!("Cannot write patch_plan.json: {}", e))?;
    included_reports.push("patch_plan.json".to_string());

    // stage_manifest.json (optional — copied from stage_dir if present).
    let stage_manifest_src = stage_dir.join("stage_manifest.json");
    if stage_manifest_src.is_file() {
        let bytes = fs::read(&stage_manifest_src)
            .map_err(|e| format!("Cannot read stage_manifest.json: {}", e))?;
        fs::write(bundle_dir.join("stage_manifest.json"), &bytes)
            .map_err(|e| format!("Cannot write stage_manifest.json: {}", e))?;
        included_reports.push("stage_manifest.json".to_string());
    } else {
        warnings.push("stage_manifest_not_found".to_string());
    }

    // apply_report.json (optional — copied from stage_dir if present).
    let apply_report_src = stage_dir.join("apply_report.json");
    if apply_report_src.is_file() {
        let bytes = fs::read(&apply_report_src)
            .map_err(|e| format!("Cannot read apply_report.json: {}", e))?;
        fs::write(bundle_dir.join("apply_report.json"), &bytes)
            .map_err(|e| format!("Cannot write apply_report.json: {}", e))?;
        included_reports.push("apply_report.json".to_string());
    } else {
        warnings.push("apply_report_not_found".to_string());
    }

    // diff_report.json (generated).
    let diff_json = serde_json::to_string_pretty(&diff_report)
        .map_err(|e| format!("Cannot serialize diff report: {}", e))?;
    fs::write(bundle_dir.join("diff_report.json"), diff_json)
        .map_err(|e| format!("Cannot write diff_report.json: {}", e))?;
    included_reports.push("diff_report.json".to_string());

    // 10. bundle_manifest.json (generated by this module).
    let created = OffsetDateTime::now_utc().format(&Rfc3339).ok();
    let file_count = bundle_files.len();
    let manifest = BundleManifest {
        bundle_format: BUNDLE_FORMAT.to_string(),
        bundle_version: BUNDLE_VERSION.to_string(),
        created,
        source_workspace_path: Some(workspace_path.to_string_lossy().to_string()),
        stage_dir: stage_dir.to_string_lossy().to_string(),
        file_count,
        files: bundle_files.clone(),
        included_reports: included_reports.clone(),
        modifies_rpf: false,
        modifies_source_workspace: false,
        exported_from_stage_only: true,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Cannot serialize bundle manifest: {}", e))?;
    let manifest_path = bundle_dir.join("bundle_manifest.json");
    fs::write(&manifest_path, manifest_json)
        .map_err(|e| format!("Cannot write bundle_manifest.json: {}", e))?;
    included_reports.push("bundle_manifest.json".to_string());

    let report_count = included_reports.len();
    let warning_count = warnings.len();

    Ok(BundleExportReport {
        safe_exported: true,
        status: BundleExportStatus::Exported,
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
        files: bundle_files,
        included_reports,
        blocked: Vec::new(),
        warnings,
        summary: BundleSummary {
            file_count,
            report_count,
            blocked_count: 0,
            warning_count,
        },
        modifies_rpf: false,
        modifies_source_workspace: false,
        exported_from_stage_only: true,
    })
}
