//! Shared policy for recognizing stage metadata / report files.
//!
//! `stage`, `diff-stage`, and `export-bundle` all walk a stage directory. The
//! directory legitimately contains generated report/metadata files alongside the
//! patched game assets. Those metadata files must never be treated as patched
//! assets (diffed against the workspace, or copied into a bundle's `files/`).
//! This module is the single source of truth for that exclusion policy.

/// Known metadata / report filenames written into a stage directory by the
/// staged-patch pipeline. Compared case-insensitively against the basename.
pub const STAGE_METADATA_FILES: &[&str] = &[
    "stage_manifest.json",
    "apply_report.json",
    "diff_report.json",
    "bundle_manifest.json",
    "patch_plan.json",
    "export_report.json",
];

/// Dedicated report/metadata directory names. Any file nested under one of these
/// directories (at any depth) is treated as metadata, not a patched asset.
pub const STAGE_METADATA_DIRS: &[&str] = &["reports", "_reports", "metadata", "_metadata"];

/// Returns `true` when `relative_path` denotes a stage metadata/report file that
/// must be excluded from asset scanning, diffing, and bundle `files/` copying.
///
/// Matching is path-separator agnostic (accepts `/` and `\`) and
/// case-insensitive. A path is metadata when either:
/// - its basename is one of [`STAGE_METADATA_FILES`], or
/// - any of its directory components is one of [`STAGE_METADATA_DIRS`].
pub fn is_stage_metadata_file(relative_path: &str) -> bool {
    let normalized = relative_path.replace('\\', "/");
    let trimmed = normalized.trim_matches('/');
    if trimmed.is_empty() {
        return false;
    }

    let components: Vec<&str> = trimmed.split('/').filter(|c| !c.is_empty()).collect();
    let Some((basename, dirs)) = components.split_last() else {
        return false;
    };

    if STAGE_METADATA_FILES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(basename))
    {
        return true;
    }

    dirs.iter().any(|dir| {
        STAGE_METADATA_DIRS
            .iter()
            .any(|meta| meta.eq_ignore_ascii_case(dir))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_metadata_helper_matches_known_report_files() {
        // Known report/metadata files at the stage root.
        assert!(is_stage_metadata_file("stage_manifest.json"));
        assert!(is_stage_metadata_file("apply_report.json"));
        assert!(is_stage_metadata_file("diff_report.json"));
        assert!(is_stage_metadata_file("bundle_manifest.json"));
        assert!(is_stage_metadata_file("patch_plan.json"));

        // Separator and case agnostic.
        assert!(is_stage_metadata_file("APPLY_REPORT.JSON"));
        assert!(is_stage_metadata_file("\\diff_report.json"));

        // Dedicated metadata/report directories (any depth).
        assert!(is_stage_metadata_file("reports/anything.json"));
        assert!(is_stage_metadata_file("_reports/run/notes.txt"));
        assert!(is_stage_metadata_file("metadata/info.json"));
        assert!(is_stage_metadata_file("common/_metadata/x.bin"));

        // Real patched assets must NOT be flagged.
        assert!(!is_stage_metadata_file("common/data/visualsettings.dat"));
        assert!(!is_stage_metadata_file("common/data/timecycle/w_clear.xml"));
        // A similarly-named-but-distinct file is not metadata.
        assert!(!is_stage_metadata_file("common/data/apply_report.dat"));
        assert!(!is_stage_metadata_file(""));
    }
}
