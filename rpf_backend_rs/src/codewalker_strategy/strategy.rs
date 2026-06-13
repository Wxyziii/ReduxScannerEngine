use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

pub const SELECTED_WRITER_ROUTE: &str = "CodeWalker.API";
pub const PLANNED_BASE_URL_DEFAULT: &str = "http://localhost:5555";

fn capability(name: &str, description: &str) -> CodeWalkerPlannedCapability {
    CodeWalkerPlannedCapability {
        name: name.to_string(),
        implemented: false,
        description: description.to_string(),
    }
}

fn gate(name: &str, description: &str) -> CodeWalkerSafetyGate {
    CodeWalkerSafetyGate {
        name: name.to_string(),
        satisfied: false,
        required: true,
        description: description.to_string(),
    }
}

fn milestone(id: &str, title: &str, description: &str) -> CodeWalkerMilestonePlan {
    CodeWalkerMilestonePlan {
        id: id.to_string(),
        title: title.to_string(),
        implemented: false,
        description: description.to_string(),
    }
}

/// Build the static CodeWalker writer-strategy report. Deterministic: takes no
/// inputs, reads no files, executes nothing. Locks CodeWalker.API as the future
/// writer route while keeping every write/exec/detection flag `false`.
pub fn build_codewalker_strategy_report() -> Result<CodeWalkerStrategyReport, String> {
    // Active adapter facts come from the real, safe adapter — never CodeWalker.
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let active_adapter_is_null = !adapter.capabilities().can_write_archive;

    let planned_capabilities = vec![
        capability(
            "detect_codewalker_api",
            "Detect a local CodeWalker.API endpoint (planned T0.6.0).",
        ),
        capability(
            "readiness_probe",
            "Probe CodeWalker.API readiness/health before any work (planned T0.6.1).",
        ),
        capability(
            "search_resolve_entry",
            "Resolve an archive-relative entry to a CodeWalker target (planned T0.6.2).",
        ),
        capability(
            "dry_replace_plan",
            "Plan a file replace without applying it (planned T0.6.3).",
        ),
        capability(
            "replace_on_copied_test_archive",
            "Apply a replace on a COPIED test archive only (planned T0.6.4).",
        ),
        capability(
            "post_write_verify_rollback",
            "Verify the written archive and roll back on mismatch (planned T0.6.5).",
        ),
    ];

    let required_safety_gates = vec![
        gate("backup_rpf_verified", "A hash-verified backup exists."),
        gate("probe_rpf_successful", "Read-only target probe succeeded."),
        gate("entry_manifest_built", "An entry manifest was built."),
        gate("write_readiness_checked", "Write readiness was evaluated."),
        gate(
            "writer_permission_token_present",
            "A manual writer-permission token is present.",
        ),
        gate(
            "copied_test_archive_only",
            "The operation targets a COPIED test archive only.",
        ),
        gate(
            "codewalker_api_detected",
            "CodeWalker.API was detected (T0.6.0).",
        ),
        gate(
            "codewalker_replace_endpoint_available",
            "The CodeWalker.API replace endpoint is available.",
        ),
        gate(
            "codewalker_target_resolution_successful",
            "The target entry resolved via CodeWalker.API.",
        ),
        gate(
            "manual_confirmation_required",
            "Explicit human confirmation is required.",
        ),
        gate(
            "rollback_restore_available",
            "A rollback/restore path is available.",
        ),
        gate(
            "post_write_verification_required",
            "Post-write verification is required.",
        ),
        gate(
            "codewalker_execution_not_enabled_yet",
            "CodeWalker execution is disabled in the current milestone.",
        ),
    ];

    let milestone_plan = vec![
        milestone(
            "T0.6.0",
            "CodeWalker.API Detection Adapter",
            "Detect a local CodeWalker.API endpoint (informational only).",
        ),
        milestone(
            "T0.6.1",
            "CodeWalker.API Readiness Probe",
            "Probe CodeWalker.API readiness before any planning.",
        ),
        milestone(
            "T0.6.2",
            "CodeWalker Search/Resolve Plan",
            "Resolve archive-relative entries to CodeWalker targets.",
        ),
        milestone(
            "T0.6.3",
            "CodeWalker Dry Replace Plan",
            "Plan replaces without applying them.",
        ),
        milestone(
            "T0.6.4",
            "CodeWalker Copied-Test-Archive Execution Gate",
            "Decide whether a future replace attempt on a copied test archive is eligible.",
        ),
        milestone(
            "T0.6.5",
            "Controlled CodeWalker Replace Apply on copied test archive",
            "Send scoped /api/replace-file requests for copied test archives only.",
        ),
        milestone(
            "T0.6.6",
            "Post-write verification and rollback planning",
            "Verify written output and plan rollback for copied test archives.",
        ),
        milestone(
            "T0.6.7",
            "Controlled rollback execution from backup",
            "Restore a copied test archive from its verified backup, heavily gated.",
        ),
        milestone(
            "T0.6.8",
            "Real Copied Archive Manual Test Harness",
            "Prepare a safe copied-test-archive command checklist/script; no archive mutation.",
        ),
        milestone(
            "T0.6.9",
            "CodeWalker Live Compatibility Probe",
            "Safely check CodeWalker.API endpoint shapes/availability; no replace POST, no mutation.",
        ),
        milestone(
            "T0.6.10",
            "Real Copied Archive Test Run Coordinator",
            "Plan-first coordinator for a full copied-test replace cycle; execute mode gated, \
             no original archives.",
        ),
        milestone(
            "T0.6.11",
            "CodeWalker Test Report Normalizer",
            "Read-only summary of compatibility/readiness/resolve/dry-plan/gate/apply/verify/\
             rollback reports; no pipeline run, no HTTP, no archive modification.",
        ),
    ];

    // T0.6.0 (detection) and T0.6.1 (readiness) have shipped.
    let mut milestone_plan = milestone_plan;
    for m in milestone_plan.iter_mut() {
        if m.id == "T0.6.0"
            || m.id == "T0.6.1"
            || m.id == "T0.6.2"
            || m.id == "T0.6.3"
            || m.id == "T0.6.4"
            || m.id == "T0.6.5"
            || m.id == "T0.6.6"
            || m.id == "T0.6.7"
            || m.id == "T0.6.8"
            || m.id == "T0.6.9"
            || m.id == "T0.6.10"
            || m.id == "T0.6.11"
        {
            m.implemented = true;
        }
    }

    let blocked_items = vec![
        CodeWalkerBlockedItem {
            component: "writer".to_string(),
            reason: "The real RPF writer is not implemented.".to_string(),
            block_type: "real_rpf_writer_not_implemented".to_string(),
        },
        CodeWalkerBlockedItem {
            component: "parser".to_string(),
            reason: "Native RPF parsing is not implemented.".to_string(),
            block_type: "native_rpf_parser_not_implemented".to_string(),
        },
        CodeWalkerBlockedItem {
            component: "codewalker".to_string(),
            reason: "CodeWalker execution is not implemented and not enabled.".to_string(),
            block_type: "codewalker_execution_not_enabled".to_string(),
        },
        CodeWalkerBlockedItem {
            component: "adapter".to_string(),
            reason: "The active adapter is NullRpfAdapter and cannot write.".to_string(),
            block_type: "active_adapter_cannot_write".to_string(),
        },
    ];

    let summary = CodeWalkerStrategySummary {
        planned_capability_count: planned_capabilities.len(),
        required_safety_gate_count: required_safety_gates.len(),
        milestone_count: milestone_plan.len(),
        blocked_count: blocked_items.len(),
        writer_allowed_now: false,
    };

    Ok(CodeWalkerStrategyReport {
        status: CodeWalkerStrategyStatus::RouteLocked,
        selected_writer_route: SELECTED_WRITER_ROUTE.to_string(),
        selected_writer_route_locked: true,
        selected_route: CodeWalkerSelectedRoute {
            name: SELECTED_WRITER_ROUTE.to_string(),
            locked: true,
            planned_base_url_default: PLANNED_BASE_URL_DEFAULT.to_string(),
        },
        active_adapter_name,
        active_adapter_is_null,
        // T0.6.0 detect; T0.6.1 readiness; T0.6.2 search/target resolution.
        codewalker_detection_implemented: true,
        codewalker_readiness_implemented: true,
        codewalker_search_resolution_implemented: true,
        codewalker_dry_replace_plan_implemented: true,
        codewalker_execution_gate_implemented: true,
        // Scoped executor shipped (T0.6.5): replace apply for copied test archives
        // only. Global execution/writing stays disabled.
        codewalker_replace_apply_implemented: true,
        // Post-write verification + rollback planning shipped (T0.6.6).
        codewalker_post_write_verification_implemented: true,
        // Controlled rollback restore shipped (T0.6.7): copied test archives only.
        codewalker_rollback_restore_implemented: true,
        // Real copied-archive manual test harness shipped (T0.6.8): plan/checklist
        // first, no archive mutation in plan mode, original game paths blocked.
        codewalker_manual_harness_implemented: true,
        // Live compatibility probe shipped (T0.6.9): safe endpoint shape/availability
        // checks only; no replace POST, no archive mutation.
        codewalker_compatibility_probe_implemented: true,
        // Real copied-archive test-run coordinator shipped (T0.6.10): plan-first,
        // execute mode gated, original game paths blocked, no auto rollback.
        codewalker_copied_archive_test_run_implemented: true,
        // Read-only test report normalizer shipped (T0.6.11): folds the pipeline
        // reports into one verdict; no pipeline run, no HTTP, no archive mutation.
        codewalker_test_summary_implemented: true,
        codewalker_execution_implemented: false,
        codewalker_write_allowed_now: false,
        writer_allowed_now: false,
        real_writer_implemented: false,
        native_parser_implemented: false,
        external_tool_execution_allowed: false,
        planned_base_url_default: PLANNED_BASE_URL_DEFAULT.to_string(),
        planned_capabilities,
        required_safety_gates,
        milestone_plan,
        blocked_items,
        summary,
    })
}
