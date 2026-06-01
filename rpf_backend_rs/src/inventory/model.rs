use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InventoryScanStatus {
    Ok,
    Empty,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFile {
    /// Normalized relative path from workspace root (forward slashes, no leading slash).
    pub path: String,
    pub extension: String,
    pub size_bytes: u64,
    pub is_text_like: bool,
    /// SHA-256 hex digest; only computed for files <= 1 MiB.
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryMissingTarget {
    pub target_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySummary {
    pub total_files: usize,
    pub text_like_files: usize,
    pub binary_like_files: usize,
    /// Sorted list of unique file extensions found in the workspace.
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReport {
    pub status: InventoryScanStatus,
    /// Workspace root path as provided to the scanner.
    pub workspace_path: String,
    pub files: Vec<InventoryFile>,
    pub missing_targets: Vec<InventoryMissingTarget>,
    pub summary: InventorySummary,
}
