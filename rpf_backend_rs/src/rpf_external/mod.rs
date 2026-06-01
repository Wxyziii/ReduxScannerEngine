pub mod model;
pub mod tools;

#[cfg(test)]
mod tests;

use model::*;

/// Build the external-tool adapter planning report.
///
/// This detects known tools on PATH (informational only), describes their
/// theoretical capabilities, and marks every mutation / automatic-execution path
/// as blocked. It NEVER executes a tool, NEVER opens a real RPF file, and NEVER
/// modifies any file.
pub fn build_external_tool_adapter_plan() -> Result<ExternalToolAdapterPlan, String> {
    let entries = tools::plan_known_tools();

    let tools_checked = entries.len();
    let tools_found = entries.iter().filter(|e| e.detection.found).count();
    let external_tools_detected = tools_found > 0;

    // Aggregate blocked items across all tools.
    let mut blocked: Vec<ExternalToolBlockedItem> = Vec::new();
    for e in &entries {
        blocked.extend(e.blocked.iter().cloned());
    }
    let blocked_count = blocked.len();

    // Safety gates — all conservative for this milestone.
    let safety_gates = vec![
        ExternalToolSafetyGate {
            name: "no_automatic_tool_execution".to_string(),
            passed: true,
            detail: "No external tool is executed automatically; detection is PATH lookup only."
                .to_string(),
        },
        ExternalToolSafetyGate {
            name: "no_archive_mutation".to_string(),
            passed: true,
            detail: "No external tool is permitted to modify or write an archive.".to_string(),
        },
        ExternalToolSafetyGate {
            name: "manual_user_action_required_for_future_writes".to_string(),
            passed: true,
            detail: "Any future external write path requires explicit manual user action, \
                     plus backup-rpf, probe-rpf, plan-rpf-write gates, and a trusted adapter mode."
                .to_string(),
        },
        ExternalToolSafetyGate {
            name: "null_adapter_remains_active".to_string(),
            passed: true,
            detail: "NullRpfAdapter remains the active adapter; this is planning only.".to_string(),
        },
    ];

    Ok(ExternalToolAdapterPlan {
        status: ExternalToolAdapterStatus::Planned,
        adapter_name: "external_tool_adapter_plan".to_string(),
        external_tools_detected,
        can_use_external_tools_automatically: false,
        can_modify_archive: false,
        can_write_archive: false,
        can_parse_internals: false,
        safe_mode_only: true,
        manual_user_action_required: true,
        tools: entries,
        safety_gates,
        blocked,
        summary: ExternalToolSummary {
            tools_checked,
            tools_found,
            blocked_count,
            external_tools_detected,
            can_use_external_tools_automatically: false,
            can_write_archive: false,
            safe_mode_only: true,
        },
        modifies_files: false,
    })
}
