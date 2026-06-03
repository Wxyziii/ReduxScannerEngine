use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunReport {
    pub safe_to_apply: bool,
    pub status: DryRunStatus,
    pub targets: Vec<DryRunTarget>,
    pub blocked: Vec<DryRunBlockedItem>,
    pub warnings: Vec<DryRunWarning>,
    pub missing_targets: Vec<DryRunMissingTarget>,
    pub summary: DryRunSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunMissingTarget {
    pub target_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DryRunStatus {
    AllClear,
    HasWarnings,
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunTarget {
    pub operation_id: String,
    pub file_path: String,
    pub would_change: bool,
    pub validators_planned: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunBlockedItem {
    pub operation_id: String,
    pub file_path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunWarning {
    pub operation_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunSummary {
    pub total_operations: usize,
    pub allowed_operations: usize,
    pub blocked_operations: usize,
    pub warning_count: usize,
    pub missing_target_count: usize,
}
