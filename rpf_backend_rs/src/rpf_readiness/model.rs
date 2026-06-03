use serde::Serialize;

use crate::rpf_adapter::model::RpfAdapterInfoReport;
use crate::rpf_external::model::ExternalToolAdapterPlan;
use crate::rpf_probe::model::RpfProbeReport;
use crate::rpf_writer::model::RpfWritePlan;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfWriteReadinessStatus {
    /// A readiness report was produced, but writing is not permitted.
    NotReady,
    /// Inputs were invalid enough that a complete readiness picture is blocked.
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfReadinessSeverity {
    Info,
    Warning,
    Blocking,
}

/// A single readiness gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfReadinessGate {
    pub name: String,
    pub passed: bool,
    pub severity: RpfReadinessSeverity,
    pub message: String,
}

/// A reason the bundle is not ready to write.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfReadinessBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A compact, glanceable status for one input component.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfReadinessComponent {
    pub name: String,
    pub present: bool,
    pub ok: bool,
    /// One of `ok`, `missing`, `warning`, `blocked`.
    pub status: String,
    pub detail: String,
}

/// The six input components combined by the readiness report.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfReadinessComponents {
    pub bundle: RpfReadinessComponent,
    pub write_plan: RpfReadinessComponent,
    pub backup: RpfReadinessComponent,
    pub probe: RpfReadinessComponent,
    pub adapter: RpfReadinessComponent,
    pub external_tools: RpfReadinessComponent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfReadinessSummary {
    pub total_gates: usize,
    pub passed_gates: usize,
    pub blocking_gates: usize,
    pub blocked_count: usize,
    pub ready_to_write: bool,
}

/// Unified, read-only pre-write decision object.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriteReadinessReport {
    pub status: RpfWriteReadinessStatus,

    pub bundle_dir: String,
    pub target_rpf: String,
    pub backup_report_path: Option<String>,

    /// Always `false` in this milestone.
    pub ready_to_write: bool,

    pub components: RpfReadinessComponents,
    pub gates: Vec<RpfReadinessGate>,
    pub blocked: Vec<RpfReadinessBlockedItem>,
    pub summary: RpfReadinessSummary,

    // ── Embedded source reports (full detail) ───────────────────────────────
    pub write_plan: RpfWritePlan,
    pub probe: Option<RpfProbeReport>,
    pub adapter_info: RpfAdapterInfoReport,
    pub external_tool_plan: ExternalToolAdapterPlan,

    // ── Mirrored safety facts ───────────────────────────────────────────────
    pub modifies_target_archive: bool,
    pub real_writer_implemented: bool,
    pub native_parser_implemented: bool,
}
