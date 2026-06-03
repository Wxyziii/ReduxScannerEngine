use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BundleExportStatus {
    Exported,
    Blocked,
}

/// A single staged file copied into the bundle.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleFile {
    /// Relative path within the bundle's `files/` tree (forward-slash normalized).
    pub relative_path: String,
    /// Absolute path of the staged source file that was copied.
    pub source_staged_path: String,
    /// Absolute path of the copy written inside the bundle.
    pub exported_path: String,
    pub size_bytes: u64,
    pub extension: String,
    /// SHA-256 hex of the exported copy; `None` if the file exceeds 1 MiB.
    pub sha256: Option<String>,
}

/// An item that prevented (or limited) the export.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleSummary {
    pub file_count: usize,
    pub report_count: usize,
    pub blocked_count: usize,
    pub warning_count: usize,
}

/// Written as `bundle_manifest.json` at the bundle root.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub bundle_format: String,
    pub bundle_version: String,
    /// RFC3339 creation timestamp; `None` if the system clock was unavailable.
    pub created: Option<String>,
    pub source_workspace_path: Option<String>,
    pub stage_dir: String,
    pub file_count: usize,
    pub files: Vec<BundleFile>,
    /// Report file names included at the bundle root (e.g. `patch_plan.json`).
    pub included_reports: Vec<String>,
    // Safety flags — these are always the same constant values for this milestone.
    pub modifies_rpf: bool,
    pub modifies_source_workspace: bool,
    pub exported_from_stage_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleExportReport {
    /// `true` only when the bundle was fully written with no blocking issues.
    pub safe_exported: bool,
    pub status: BundleExportStatus,
    /// Absolute path to the bundle directory.
    pub bundle_dir: String,
    /// Absolute path to `bundle_manifest.json`; `None` when export was blocked.
    pub manifest_path: Option<String>,
    pub files: Vec<BundleFile>,
    pub included_reports: Vec<String>,
    pub blocked: Vec<BundleBlockedItem>,
    /// Non-fatal warnings (e.g. `apply_report_not_found`).
    pub warnings: Vec<String>,
    pub summary: BundleSummary,
    // Safety flags mirrored onto the top-level report for easy consumption.
    pub modifies_rpf: bool,
    pub modifies_source_workspace: bool,
    pub exported_from_stage_only: bool,
}
