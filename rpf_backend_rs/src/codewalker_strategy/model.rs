use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerStrategyStatus {
    /// The CodeWalker.API writer route is locked in; writing stays disabled.
    RouteLocked,
}

/// The selected future writer route. Locked to CodeWalker.API in this milestone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSelectedRoute {
    pub name: String,
    pub locked: bool,
    pub planned_base_url_default: String,
}

/// A capability CodeWalker.API is planned to provide in a later milestone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerPlannedCapability {
    pub name: String,
    /// Always `false` this milestone — planned only, not implemented.
    pub implemented: bool,
    pub description: String,
}

/// A safety gate that must pass before any future CodeWalker write.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSafetyGate {
    pub name: String,
    /// Always `false` this milestone — none satisfied yet.
    pub satisfied: bool,
    pub required: bool,
    pub description: String,
}

/// One planned T0.6.x milestone in the CodeWalker route.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerMilestonePlan {
    pub id: String,
    pub title: String,
    /// Always `false` this milestone — future work.
    pub implemented: bool,
    pub description: String,
}

/// A reason CodeWalker writing is not (and cannot yet be) enabled.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerStrategySummary {
    pub planned_capability_count: usize,
    pub required_safety_gate_count: usize,
    pub milestone_count: usize,
    pub blocked_count: usize,
    pub writer_allowed_now: bool,
}

/// Static, deterministic CodeWalker writer-strategy report. Locks CodeWalker.API
/// as the selected future writer route WITHOUT implementing, detecting, calling,
/// or executing CodeWalker, and WITHOUT enabling any RPF write.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerStrategyReport {
    pub status: CodeWalkerStrategyStatus,

    pub selected_writer_route: String,
    pub selected_writer_route_locked: bool,
    pub selected_route: CodeWalkerSelectedRoute,

    pub active_adapter_name: String,
    pub active_adapter_is_null: bool,

    pub codewalker_detection_implemented: bool,
    pub codewalker_execution_implemented: bool,
    pub codewalker_write_allowed_now: bool,

    pub writer_allowed_now: bool,
    pub real_writer_implemented: bool,
    pub native_parser_implemented: bool,
    pub external_tool_execution_allowed: bool,

    pub planned_base_url_default: String,

    pub planned_capabilities: Vec<CodeWalkerPlannedCapability>,
    pub required_safety_gates: Vec<CodeWalkerSafetyGate>,
    pub milestone_plan: Vec<CodeWalkerMilestonePlan>,
    pub blocked_items: Vec<CodeWalkerBlockedItem>,
    pub summary: CodeWalkerStrategySummary,
}
