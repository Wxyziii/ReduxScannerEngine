use super::model::*;

/// The contract every RPF adapter must implement — present and future.
///
/// In this milestone the only implementation is [`super::null_adapter::NullRpfAdapter`],
/// which is safe-mode only: it never opens, parses, or modifies an archive, and
/// `execute_operation` performs no side effects whatsoever.
///
/// Any future native or external-tool adapter must implement this same contract
/// and is still required to pass the existing safety gates (see `plan-rpf-write`)
/// before any real write can occur.
pub trait RpfAdapter {
    /// Stable adapter name (snake_case).
    fn name(&self) -> &'static str;

    /// What kind of adapter this is.
    fn kind(&self) -> RpfAdapterKind;

    /// Declared capabilities of this adapter.
    fn capabilities(&self) -> RpfAdapterCapabilities;

    /// Plan an operation without performing any side effects.
    fn plan_operation(&self, operation: RpfAdapterOperation) -> RpfAdapterOperationPlan;

    /// "Execute" an operation. Implementations in this milestone MUST NOT modify
    /// anything; this exists to lock in the contract shape.
    fn execute_operation(&self, operation: RpfAdapterOperation) -> RpfAdapterOperationResult;
}

/// Build a full capability report for an adapter by planning every operation.
/// Pure inspection — no archive is opened or modified.
pub fn build_adapter_info_report(adapter: &dyn RpfAdapter) -> RpfAdapterInfoReport {
    let capabilities = adapter.capabilities();

    let mut operation_plans: Vec<RpfAdapterOperationPlan> = Vec::new();
    let mut blocked: Vec<RpfAdapterBlockedItem> = Vec::new();

    for op in RpfAdapterOperation::all() {
        let plan = adapter.plan_operation(*op);
        blocked.extend(plan.blocked.iter().cloned());
        operation_plans.push(plan);
    }

    let operation_count = operation_plans.len();
    let supported_operation_count = operation_plans.iter().filter(|p| p.supported).count();
    let blocked_operation_count = operation_plans.iter().filter(|p| !p.supported).count();

    let note = format!(
        "Current adapter is `{}` ({:?}). It is safe-mode only: no native RPF parsing \
         or writing is implemented. probe-rpf provides metadata/hash only; all \
         list/extract/replace/write operations are blocked as not implemented.",
        adapter.name(),
        adapter.kind(),
    );

    RpfAdapterInfoReport {
        adapter_name: adapter.name().to_string(),
        adapter_kind: adapter.kind(),
        capabilities,
        operation_plans,
        blocked,
        summary: RpfAdapterSummary {
            operation_count,
            supported_operation_count,
            blocked_operation_count,
            safe_mode_only: adapter.capabilities().safe_mode_only,
        },
        native_adapter_implemented: false,
        note,
        modifies_archive: false,
        // Informational external-tool planning. This never changes the active
        // adapter and never executes a tool.
        external_tool_plan: crate::rpf_external::build_external_tool_adapter_plan()
            .unwrap_or_else(|_| panic!("external tool planning must not fail")),
    }
}
