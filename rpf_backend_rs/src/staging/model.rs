use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    AllClear,
    Blocked,
}

/// A single file that was (or would have been) staged.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageFile {
    /// Relative path from the workspace root (mirrors the PatchPlan operation path).
    pub source_path: String,
    /// Relative path within the stage directory (same as source_path).
    pub staged_path: String,
    pub source_abs: String,
    pub staged_abs: String,
    pub size_bytes: u64,
    pub extension: String,
    /// SHA-256 hex of the staged copy; None if file exceeds 1 MiB.
    pub sha256: Option<String>,
    /// `"staged"` on success.
    pub status: String,
}

/// An operation or target that prevented staging.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageSummary {
    pub total_targets: usize,
    pub staged_count: usize,
    pub blocked_count: usize,
}

/// Written as `stage_manifest.json` inside the stage directory.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageManifest {
    pub plan_path: String,
    pub workspace_path: String,
    pub stage_dir: String,
    pub safe_to_stage: bool,
    pub files: Vec<StageFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageReport {
    pub safe_to_stage: bool,
    pub status: StageStatus,
    pub files: Vec<StageFile>,
    pub blocked: Vec<StageBlockedItem>,
    pub summary: StageSummary,
    /// Absolute path to `stage_manifest.json`; `None` when staging was blocked.
    pub manifest_path: Option<String>,
}
