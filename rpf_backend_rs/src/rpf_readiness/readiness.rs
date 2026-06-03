use super::model::*;
use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::rpf_adapter::contract::build_adapter_info_report;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;
use crate::rpf_external::build_external_tool_adapter_plan;
use crate::rpf_probe::model::RpfProbeStatus;
use crate::rpf_probe::probe::probe_rpf_archive;
use crate::rpf_writer::model::RpfWritePlanStatus;
use crate::rpf_writer::plan::build_rpf_write_plan;

// ── Backup report (read-only deserialize view) ──────────────────────────────

/// Minimal view of an `RpfBackupReport` JSON (camelCase keys). Unknown fields
/// are ignored. We only read it — never modify it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupReportView {
    #[serde(default)]
    target_archive_path: Option<String>,
    #[serde(default)]
    hash_verified: bool,
    #[serde(default)]
    safe_for_future_write: bool,
    #[serde(default)]
    status: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn gate(
    name: &str,
    passed: bool,
    severity: RpfReadinessSeverity,
    message: &str,
) -> RpfReadinessGate {
    RpfReadinessGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

fn component(
    name: &str,
    present: bool,
    ok: bool,
    status: &str,
    detail: &str,
) -> RpfReadinessComponent {
    RpfReadinessComponent {
        name: name.to_string(),
        present,
        ok,
        status: status.to_string(),
        detail: detail.to_string(),
    }
}

/// Whether a write-plan gate of the given name passed.
fn write_plan_gate_passed(plan: &crate::rpf_writer::model::RpfWritePlan, name: &str) -> bool {
    plan.safety_gates
        .iter()
        .find(|g| g.gate == name)
        .map(|g| g.passed)
        .unwrap_or(false)
}

/// Outcome of reading the optional backup report.
struct BackupOutcome {
    provided: bool,
    present: bool,
    verified: bool,
    target_matches: bool,
    detail: String,
}

fn read_backup_report(backup_report_path: Option<&Path>, target_rpf: &Path) -> BackupOutcome {
    let Some(path) = backup_report_path else {
        return BackupOutcome {
            provided: false,
            present: false,
            verified: false,
            target_matches: false,
            detail: "No backup report supplied; run `backup-rpf` for a verified backup."
                .to_string(),
        };
    };

    if !path.is_file() {
        return BackupOutcome {
            provided: true,
            present: false,
            verified: false,
            target_matches: false,
            detail: format!("Backup report not found: {}", path.display()),
        };
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return BackupOutcome {
                provided: true,
                present: false,
                verified: false,
                target_matches: false,
                detail: format!("Backup report unreadable: {}", e),
            };
        }
    };
    let view: BackupReportView = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return BackupOutcome {
                provided: true,
                present: false,
                verified: false,
                target_matches: false,
                detail: format!("Backup report could not be parsed: {}", e),
            };
        }
    };

    let verified = view.hash_verified
        && view.safe_for_future_write
        && view.status.as_deref() != Some("blocked");

    // If the report records a target path, check it points at the same archive.
    let target_matches = match view.target_archive_path.as_deref() {
        Some(p) => paths_equal(Path::new(p), target_rpf),
        None => true, // field absent → don't fail on mismatch
    };

    let detail = if !verified {
        "Backup report present but not verified (hashVerified/safeForFutureWrite not both true)."
            .to_string()
    } else if !target_matches {
        "Backup report is verified but its target archive path does not match --target-rpf."
            .to_string()
    } else {
        "Backup report present and verified (hash matches; safe for future write).".to_string()
    };

    BackupOutcome {
        provided: true,
        present: true,
        verified,
        target_matches,
        detail,
    }
}

/// Best-effort path equality (canonicalize when possible; fall back to literal).
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Build a unified, read-only write-readiness report. NEVER opens or modifies
/// the target archive, NEVER modifies the bundle, NEVER creates backups, and
/// NEVER executes external tools. `ready_to_write` is always `false`.
pub fn build_write_readiness_report(
    bundle_dir: &Path,
    target_rpf: &Path,
    backup_report_path: Option<&Path>,
) -> Result<RpfWriteReadinessReport, String> {
    // ── Component sources ───────────────────────────────────────────────────
    let write_plan = build_rpf_write_plan(bundle_dir, target_rpf)?;

    let probe = if target_rpf.exists() {
        Some(probe_rpf_archive(target_rpf)?)
    } else {
        None
    };

    let adapter_info = build_adapter_info_report(&NullRpfAdapter::new());
    let external_tool_plan = build_external_tool_adapter_plan()?;
    let backup = read_backup_report(backup_report_path, target_rpf);

    // ── Derived component states ────────────────────────────────────────────
    let bundle_manifest_present = write_plan_gate_passed(&write_plan, "bundle_manifest_present");
    let bundle_flags_valid = write_plan_gate_passed(&write_plan, "bundle_safety_flags_valid");
    let write_plan_built = write_plan.status == RpfWritePlanStatus::Planned;

    let probe_ok = probe
        .as_ref()
        .map(|p| p.status == RpfProbeStatus::Probed)
        .unwrap_or(false);

    let adapter_can_write = adapter_info.capabilities.can_write_archive;
    let external_mutation_allowed = external_tool_plan.can_modify_archive;

    let components = RpfReadinessComponents {
        bundle: component(
            "bundle",
            bundle_manifest_present,
            bundle_manifest_present && bundle_flags_valid,
            if bundle_manifest_present && bundle_flags_valid {
                "ok"
            } else if bundle_manifest_present {
                "warning"
            } else {
                "blocked"
            },
            "Bundle manifest + safety flags as understood by the write plan.",
        ),
        write_plan: component(
            "write_plan",
            true,
            write_plan_built,
            if write_plan_built { "ok" } else { "blocked" },
            "Planning-only RPF write plan (safeToWrite is always false).",
        ),
        backup: component(
            "backup",
            backup.present,
            backup.verified && backup.target_matches,
            if !backup.provided {
                "missing"
            } else if backup.verified && backup.target_matches {
                "ok"
            } else {
                "warning"
            },
            &backup.detail,
        ),
        probe: component(
            "probe",
            probe.is_some(),
            probe_ok,
            if probe.is_none() {
                "missing"
            } else if probe_ok {
                "ok"
            } else {
                "blocked"
            },
            "Read-only target metadata/hash probe (internals never parsed).",
        ),
        adapter: component(
            "adapter",
            true,
            true,
            "ok",
            &format!(
                "Active adapter is `{}` (safe-mode only); canWriteArchive=false.",
                adapter_info.adapter_name
            ),
        ),
        external_tools: component(
            "external_tools",
            true,
            true,
            "ok",
            "External tool plan loaded: safeModeOnly=true, automatic execution=false.",
        ),
    };

    // ── Gates ───────────────────────────────────────────────────────────────
    let mut gates: Vec<RpfReadinessGate> = Vec::new();
    let mut blocked: Vec<RpfReadinessBlockedItem> = Vec::new();

    gates.push(gate(
        "bundle_manifest_present",
        bundle_manifest_present,
        if bundle_manifest_present {
            RpfReadinessSeverity::Info
        } else {
            RpfReadinessSeverity::Blocking
        },
        if bundle_manifest_present {
            "bundle_manifest.json present and parsed."
        } else {
            "bundle_manifest.json missing or invalid."
        },
    ));
    gates.push(gate(
        "bundle_safety_flags_valid",
        bundle_flags_valid,
        if bundle_flags_valid {
            RpfReadinessSeverity::Info
        } else {
            RpfReadinessSeverity::Blocking
        },
        if bundle_flags_valid {
            "Bundle safety flags valid (stage-only, does not modify RPF/workspace)."
        } else {
            "Bundle safety flags missing or unsafe."
        },
    ));
    gates.push(gate(
        "write_plan_built",
        write_plan_built,
        if write_plan_built {
            RpfReadinessSeverity::Info
        } else {
            RpfReadinessSeverity::Blocking
        },
        if write_plan_built {
            "RPF write plan built from the bundle/target inputs."
        } else {
            "RPF write plan could not be completed from the inputs."
        },
    ));
    gates.push(gate(
        "backup_report_present_or_missing",
        backup.present,
        if backup.present {
            RpfReadinessSeverity::Info
        } else {
            RpfReadinessSeverity::Warning
        },
        &backup.detail,
    ));
    {
        // Verified only when a backup report is present, verified, and matches.
        let verified = backup.present && backup.verified && backup.target_matches;
        let severity = if verified {
            RpfReadinessSeverity::Info
        } else if backup.provided {
            RpfReadinessSeverity::Blocking
        } else {
            RpfReadinessSeverity::Warning
        };
        gates.push(gate(
            "backup_hash_verified",
            verified,
            severity,
            if verified {
                "Backup report is hash-verified and safe for future write."
            } else if backup.provided {
                "Backup report supplied but not hash-verified."
            } else {
                "No backup report supplied; run `backup-rpf` first."
            },
        ));
    }
    gates.push(gate(
        "target_probe_successful",
        probe_ok,
        if probe_ok {
            RpfReadinessSeverity::Info
        } else {
            RpfReadinessSeverity::Warning
        },
        if probe_ok {
            "Target archive probed read-only (metadata/hash captured)."
        } else {
            "Target archive not probed (missing or unreadable); probe is a read-only preflight."
        },
    ));
    gates.push(gate(
        "adapter_info_loaded",
        true,
        RpfReadinessSeverity::Info,
        "Adapter capability report loaded.",
    ));
    gates.push(gate(
        "adapter_supports_write",
        adapter_can_write,
        RpfReadinessSeverity::Blocking,
        "Active adapter (NullRpfAdapter) cannot write archives.",
    ));
    if !adapter_can_write {
        blocked.push(RpfReadinessBlockedItem {
            component: "adapter".to_string(),
            reason: "The active adapter is safe-mode only and cannot write archives.".to_string(),
            block_type: "active_adapter_cannot_write".to_string(),
        });
    }
    gates.push(gate(
        "external_tool_plan_loaded",
        true,
        RpfReadinessSeverity::Info,
        "External tool plan loaded.",
    ));
    gates.push(gate(
        "entry_manifest_available",
        true,
        RpfReadinessSeverity::Info,
        "A future-writer entry manifest can be produced with `rpf-entry-manifest \
         --bundle-dir <path>`; it is informational and does not enable writing.",
    ));
    gates.push(gate(
        "external_archive_mutation_allowed",
        external_mutation_allowed,
        RpfReadinessSeverity::Blocking,
        "External tool archive mutation is not allowed.",
    ));
    if !external_mutation_allowed {
        blocked.push(RpfReadinessBlockedItem {
            component: "external_tools".to_string(),
            reason: "No external tool is permitted to modify or write an archive.".to_string(),
            block_type: "external_archive_mutation_not_allowed".to_string(),
        });
    }
    gates.push(gate(
        "real_rpf_writer_implemented",
        false,
        RpfReadinessSeverity::Blocking,
        "The real RPF writer is not implemented.",
    ));
    blocked.push(RpfReadinessBlockedItem {
        component: "write_plan".to_string(),
        reason: "The real RPF writer is not implemented.".to_string(),
        block_type: "real_rpf_writer_not_implemented".to_string(),
    });
    gates.push(gate(
        "native_rpf_parser_implemented",
        false,
        RpfReadinessSeverity::Blocking,
        "Native RPF parsing is not implemented.",
    ));
    blocked.push(RpfReadinessBlockedItem {
        component: "probe".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    gates.push(gate(
        "manual_confirmation_required",
        false,
        RpfReadinessSeverity::Blocking,
        "Explicit human confirmation is required before any real archive write.",
    ));

    // ── Summary / status ────────────────────────────────────────────────────
    let total_gates = gates.len();
    let passed_gates = gates.iter().filter(|g| g.passed).count();
    let blocking_gates = gates
        .iter()
        .filter(|g| g.severity == RpfReadinessSeverity::Blocking && !g.passed)
        .count();
    let blocked_count = blocked.len();

    // ready_to_write is ALWAYS false in this milestone.
    let ready_to_write = false;
    let status = if bundle_manifest_present && bundle_flags_valid && write_plan_built {
        RpfWriteReadinessStatus::NotReady
    } else {
        RpfWriteReadinessStatus::Blocked
    };

    Ok(RpfWriteReadinessReport {
        status,
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        target_rpf: target_rpf.to_string_lossy().to_string(),
        backup_report_path: backup_report_path.map(|p| p.to_string_lossy().to_string()),
        ready_to_write,
        components,
        gates,
        blocked,
        summary: RpfReadinessSummary {
            total_gates,
            passed_gates,
            blocking_gates,
            blocked_count,
            ready_to_write,
        },
        write_plan,
        probe,
        adapter_info,
        external_tool_plan,
        modifies_target_archive: false,
        real_writer_implemented: false,
        native_parser_implemented: false,
    })
}
