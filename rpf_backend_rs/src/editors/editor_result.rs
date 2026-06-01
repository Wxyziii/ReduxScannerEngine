use super::editor_contract::SafetyResult;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EditorOperationResult {
    pub ok: bool,
    pub mode: String,
    #[serde(rename = "operationId")]
    pub operation_id: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "wouldChange")]
    pub would_change: bool,
    #[serde(rename = "wouldCreateBackup")]
    pub would_create_backup: bool,
    #[serde(rename = "validatorsPlanned")]
    pub validators_planned: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub summary: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct EditorBatchResult {
    pub ok: bool,
    pub results: Vec<EditorOperationResult>,
    pub safety: SafetyResult,
}
