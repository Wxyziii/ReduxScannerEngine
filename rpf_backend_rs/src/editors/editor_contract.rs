use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorOperation {
    pub id: String,
    pub phase: String,
    pub path: String,
    pub tool: String,
    #[serde(rename = "type")]
    pub op_type: String,
    #[serde(rename = "valueTarget")]
    pub value_target: Option<serde_json::Value>,
    pub intent: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(rename = "validationRequired", default)]
    pub validation_required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorPlan {
    pub version: String,
    pub operations: Vec<EditorOperation>,
    #[serde(rename = "targetFiles", default)]
    pub target_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    DryRun,
    ApplyBlockedForNow,
}

#[derive(Debug, Clone, Serialize)]
pub struct SafetyResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}
