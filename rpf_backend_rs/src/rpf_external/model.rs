use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalToolAdapterStatus {
    Planned,
    Blocked,
}

/// The external tools we model. Detection is informational only.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalToolKind {
    OpenIv,
    CodeWalker,
    SevenZip,
    PowerShell,
    Cmd,
}

/// How much we trust a tool for any future archive operation.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalToolTrustLevel {
    /// Not trusted for any archive operation.
    Untrusted,
    /// Could be trusted for narrow read-only use after explicit opt-in.
    Limited,
    /// Fully trusted (no tool qualifies in this milestone).
    Trusted,
}

/// How dangerous a tool is if it were ever driven against a real archive.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalToolRiskLevel {
    Low,
    Medium,
    High,
}

/// Result of looking a tool up on PATH. Nothing is ever executed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolDetection {
    pub tool: String,
    pub kind: ExternalToolKind,
    pub found: bool,
    /// How the tool was looked up (e.g. `path_lookup`).
    pub method: String,
    pub detail: String,
}

/// A theoretical capability a tool *might* provide in the future, plus whether it
/// is allowed right now (always `false` for anything that mutates an archive).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolCapability {
    pub name: String,
    /// The tool could theoretically do this in a future, trusted integration.
    pub theoretical: bool,
    /// Whether this capability is permitted to run now. Always false for any
    /// write/extract/mutation or automatic execution in this milestone.
    pub allowed_now: bool,
    pub detail: String,
}

/// A pass/fail safety gate for the external-tool plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolSafetyGate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// A refused/blocked capability or path.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolBlockedItem {
    pub tool: String,
    pub operation: String,
    pub reason: String,
    pub block_type: String,
}

/// Per-tool plan entry: detection + theoretical capabilities + risk/trust.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolPlanEntry {
    pub tool: String,
    pub kind: ExternalToolKind,
    pub detection: ExternalToolDetection,
    pub trust_level: ExternalToolTrustLevel,
    pub risk_level: ExternalToolRiskLevel,
    pub capabilities: Vec<ExternalToolCapability>,
    /// Whether this tool is allowed to run any operation now. Always false.
    pub allowed_now: bool,
    /// Any future external write path requires a manual user action.
    pub manual_user_action_required: bool,
    pub blocked: Vec<ExternalToolBlockedItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolSummary {
    pub tools_checked: usize,
    pub tools_found: usize,
    pub blocked_count: usize,
    pub external_tools_detected: bool,
    pub can_use_external_tools_automatically: bool,
    pub can_write_archive: bool,
    pub safe_mode_only: bool,
}

/// The full external-tool adapter planning report.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolAdapterPlan {
    pub status: ExternalToolAdapterStatus,
    pub adapter_name: String,

    pub external_tools_detected: bool,
    pub can_use_external_tools_automatically: bool,
    pub can_modify_archive: bool,
    pub can_write_archive: bool,
    pub can_parse_internals: bool,
    pub safe_mode_only: bool,
    /// Any future external write path requires an explicit manual user action.
    pub manual_user_action_required: bool,

    pub tools: Vec<ExternalToolPlanEntry>,
    pub safety_gates: Vec<ExternalToolSafetyGate>,
    pub blocked: Vec<ExternalToolBlockedItem>,
    pub summary: ExternalToolSummary,

    /// This planning command never modifies any file (besides an optional report).
    pub modifies_files: bool,
}
