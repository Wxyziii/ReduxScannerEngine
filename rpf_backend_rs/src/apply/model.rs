use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    AllApplied,
    Blocked,
}

/// Result for one text operation within a file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOperationResult {
    pub operation_id: String,
    pub file_path: String,
    pub op_type: String,
    /// `"applied"` or `"rolled_back"`.
    pub status: String,
    pub changed: bool,
    pub reason: Option<String>,
    pub lines_before: Option<usize>,
    pub lines_after: Option<usize>,
}

/// Aggregated result for all operations that targeted one staged file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyFileResult {
    pub file_path: String,
    pub modified: bool,
    pub operations: Vec<ApplyOperationResult>,
    /// `true` when the file was written then restored due to a later failure.
    pub rolled_back: bool,
}

/// An operation blocked during pre-validation (nothing was written).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyBlockedItem {
    pub operation_id: String,
    pub file_path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySummary {
    pub total_operations: usize,
    pub applied_count: usize,
    pub blocked_count: usize,
    pub unsupported_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    /// `true` only when every operation applied successfully with no rollbacks.
    pub safe_applied: bool,
    pub status: ApplyStatus,
    pub file_results: Vec<ApplyFileResult>,
    pub blocked: Vec<ApplyBlockedItem>,
    pub summary: ApplySummary,
}
