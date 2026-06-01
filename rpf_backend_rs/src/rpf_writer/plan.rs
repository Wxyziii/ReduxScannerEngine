use super::model::*;
use super::safety::{gate, real_writer_not_implemented_gate};
use serde::Deserialize;
use std::fs;
use std::path::Path;

// ── Bundle manifest (read-only view) ───────────────────────────────────────────

/// Minimal deserialize view of a `bundle_manifest.json` (camelCase keys written
/// by the export module). Unknown fields are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestView {
    #[serde(default)]
    bundle_format: Option<String>,
    #[serde(default)]
    modifies_rpf: Option<bool>,
    #[serde(default)]
    modifies_source_workspace: Option<bool>,
    #[serde(default)]
    exported_from_stage_only: Option<bool>,
    #[serde(default)]
    files: Vec<ManifestFileView>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFileView {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    exported_path: String,
    #[serde(default)]
    size_bytes: u64,
    #[serde(default)]
    extension: String,
    #[serde(default)]
    sha256: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn target_archive_label(target_archive_path: &Path) -> String {
    target_archive_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown.rpf".to_string())
}

fn has_rpf_extension(target_archive_path: &Path) -> bool {
    target_archive_path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("rpf"))
        .unwrap_or(false)
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Build a planning-only RPF write plan from an exported bundle.
///
/// This NEVER opens, reads, or modifies the target archive, and NEVER modifies
/// the bundle. `safe_to_write` is always `false` in this milestone: the
/// `real_rpf_writer_not_implemented` gate is terminal.
pub fn build_rpf_write_plan(
    bundle_dir: &Path,
    target_archive_path: &Path,
) -> Result<RpfWritePlan, String> {
    let target_archive_type = target_archive_label(target_archive_path);

    let mut gates: Vec<RpfWriteSafetyGate> = Vec::new();
    let mut blocked: Vec<RpfWriteBlockedItem> = Vec::new();
    let mut files_to_replace: Vec<RpfWriteTarget> = Vec::new();

    // ── Gate: bundle_manifest_present ───────────────────────────────────────
    let manifest_path = bundle_dir.join("bundle_manifest.json");
    let manifest: Option<ManifestView> = if manifest_path.is_file() {
        match fs::read_to_string(&manifest_path) {
            Ok(content) => match serde_json::from_str::<ManifestView>(&content) {
                Ok(m) => {
                    gates.push(gate(
                        "bundle_manifest_present",
                        true,
                        GateSeverity::Info,
                        "bundle_manifest.json found and parsed.",
                    ));
                    Some(m)
                }
                Err(e) => {
                    gates.push(gate(
                        "bundle_manifest_present",
                        false,
                        GateSeverity::Blocking,
                        &format!("bundle_manifest.json could not be parsed: {}", e),
                    ));
                    blocked.push(RpfWriteBlockedItem {
                        path: manifest_path.to_string_lossy().to_string(),
                        reason: format!("invalid bundle manifest: {}", e),
                        block_type: "invalid_bundle_manifest".to_string(),
                    });
                    None
                }
            },
            Err(e) => {
                gates.push(gate(
                    "bundle_manifest_present",
                    false,
                    GateSeverity::Blocking,
                    &format!("bundle_manifest.json could not be read: {}", e),
                ));
                blocked.push(RpfWriteBlockedItem {
                    path: manifest_path.to_string_lossy().to_string(),
                    reason: format!("unreadable bundle manifest: {}", e),
                    block_type: "unreadable_bundle_manifest".to_string(),
                });
                None
            }
        }
    } else {
        gates.push(gate(
            "bundle_manifest_present",
            false,
            GateSeverity::Blocking,
            "bundle_manifest.json is missing from the bundle directory.",
        ));
        blocked.push(RpfWriteBlockedItem {
            path: manifest_path.to_string_lossy().to_string(),
            reason: "bundle_manifest.json not found".to_string(),
            block_type: "missing_bundle_manifest".to_string(),
        });
        None
    };

    // ── Gate: bundle_safety_flags_valid ─────────────────────────────────────
    let flags_valid = manifest
        .as_ref()
        .map(|m| {
            m.modifies_rpf == Some(false)
                && m.modifies_source_workspace == Some(false)
                && m.exported_from_stage_only == Some(true)
                && m.bundle_format.as_deref() == Some("redux_patch_bundle")
        })
        .unwrap_or(false);
    gates.push(gate(
        "bundle_safety_flags_valid",
        flags_valid,
        if flags_valid {
            GateSeverity::Info
        } else {
            GateSeverity::Blocking
        },
        if flags_valid {
            "Bundle reports modifiesRpf=false, modifiesSourceWorkspace=false, exportedFromStageOnly=true."
        } else {
            "Bundle safety flags are missing or unsafe; expected a stage-only redux_patch_bundle that does not modify RPF or workspace."
        },
    ));
    if manifest.is_some() && !flags_valid {
        blocked.push(RpfWriteBlockedItem {
            path: manifest_path.to_string_lossy().to_string(),
            reason: "bundle safety flags not satisfied".to_string(),
            block_type: "unsafe_bundle_flags".to_string(),
        });
    }

    // ── Gate: patch_plan_present ────────────────────────────────────────────
    let patch_plan_path = bundle_dir.join("patch_plan.json");
    let patch_plan_present = patch_plan_path.is_file();
    gates.push(gate(
        "patch_plan_present",
        patch_plan_present,
        if patch_plan_present {
            GateSeverity::Info
        } else {
            GateSeverity::Blocking
        },
        if patch_plan_present {
            "patch_plan.json found in bundle."
        } else {
            "patch_plan.json missing from bundle."
        },
    ));
    if !patch_plan_present {
        blocked.push(RpfWriteBlockedItem {
            path: patch_plan_path.to_string_lossy().to_string(),
            reason: "patch_plan.json not found".to_string(),
            block_type: "missing_patch_plan".to_string(),
        });
    }

    // ── Gate: diff_report_present ───────────────────────────────────────────
    let diff_report_path = bundle_dir.join("diff_report.json");
    let diff_report_present = diff_report_path.is_file();
    gates.push(gate(
        "diff_report_present",
        diff_report_present,
        if diff_report_present {
            GateSeverity::Info
        } else {
            GateSeverity::Blocking
        },
        if diff_report_present {
            "diff_report.json found in bundle."
        } else {
            "diff_report.json missing from bundle."
        },
    ));
    if !diff_report_present {
        blocked.push(RpfWriteBlockedItem {
            path: diff_report_path.to_string_lossy().to_string(),
            reason: "diff_report.json not found".to_string(),
            block_type: "missing_diff_report".to_string(),
        });
    }

    // ── Gate: files_present ─────────────────────────────────────────────────
    let files_dir = bundle_dir.join("files");
    // Collect targets from the manifest, but only count those that actually exist.
    if let Some(m) = manifest.as_ref() {
        for f in &m.files {
            files_to_replace.push(RpfWriteTarget {
                relative_path: f.relative_path.clone(),
                bundle_file_path: f.exported_path.clone(),
                size_bytes: f.size_bytes,
                extension: f.extension.clone(),
                sha256: f.sha256.clone(),
            });
        }
    }
    let files_present = files_dir.is_dir()
        && fs::read_dir(&files_dir)
            .map(|mut e| e.next().is_some())
            .unwrap_or(false);
    gates.push(gate(
        "files_present",
        files_present,
        if files_present {
            GateSeverity::Info
        } else {
            GateSeverity::Blocking
        },
        if files_present {
            "Bundle files/ directory exists and contains patched files."
        } else {
            "Bundle files/ directory is missing or empty."
        },
    ));
    if !files_present {
        blocked.push(RpfWriteBlockedItem {
            path: files_dir.to_string_lossy().to_string(),
            reason: "files/ missing or empty".to_string(),
            block_type: "missing_files_dir".to_string(),
        });
    }

    // ── Gate: target_archive_extension_is_rpf ───────────────────────────────
    let target_is_rpf = has_rpf_extension(target_archive_path);
    gates.push(gate(
        "target_archive_extension_is_rpf",
        target_is_rpf,
        if target_is_rpf {
            GateSeverity::Info
        } else {
            GateSeverity::Blocking
        },
        if target_is_rpf {
            "Target archive path ends with .rpf."
        } else {
            "Target archive path does not end with .rpf."
        },
    ));
    if !target_is_rpf {
        blocked.push(RpfWriteBlockedItem {
            path: target_archive_path.to_string_lossy().to_string(),
            reason: "target archive path must end with .rpf".to_string(),
            block_type: "non_rpf_target".to_string(),
        });
    }

    // ── Required-future-work gates (always describe what writing would need) ─
    gates.push(gate(
        "backup_required",
        true,
        GateSeverity::Blocking,
        "A full backup of the target archive must be created before any write. \
         Use `backup-rpf --target-rpf <path> --backup-dir <path>`; future writing \
         will require a successful, hash-verified RpfBackupReport.",
    ));
    gates.push(gate(
        "restore_plan_required",
        true,
        GateSeverity::Blocking,
        "A verified restore path must exist to roll back a failed write.",
    ));
    gates.push(gate(
        "hash_verification_required",
        true,
        GateSeverity::Blocking,
        "Written entries must be SHA-256 verified against the bundle before commit. \
         Use `probe-rpf --target-rpf <path>` to capture target metadata/hash as a \
         read-only preflight; the native RPF writer is still not implemented.",
    ));
    gates.push(gate(
        "manual_confirmation_required",
        true,
        GateSeverity::Blocking,
        "Explicit human confirmation is required before any real archive write.",
    ));

    // ── Terminal gate: real writer not implemented ──────────────────────────
    gates.push(real_writer_not_implemented_gate());
    blocked.push(RpfWriteBlockedItem {
        path: target_archive_path.to_string_lossy().to_string(),
        reason: "real RPF writer is not implemented".to_string(),
        block_type: "real_rpf_writer_not_implemented".to_string(),
    });

    // ── Backup / restore plans (descriptive) ────────────────────────────────
    let backup_plan = RpfWriteBackupPlan {
        required: true,
        strategy: "Copy the target archive to a timestamped backup before any write, \
                   and verify the copy by SHA-256 before proceeding."
            .to_string(),
        suggested_backup_path: format!("{}.bak", target_archive_path.to_string_lossy()),
    };
    let restore_plan = RpfWriteRestorePlan {
        required: true,
        strategy: "On any write error or hash mismatch, restore the archive from the \
                   verified backup and abort, leaving the original archive intact."
            .to_string(),
    };

    // ── Summary ─────────────────────────────────────────────────────────────
    let gate_count = gates.len();
    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| g.severity == GateSeverity::Blocking && !g.passed)
        .count();
    let target_count = files_to_replace.len();
    let blocked_count = blocked.len();

    // safe_to_write is ALWAYS false in this milestone.
    let safe_to_write = false;
    // Status reflects whether the bundle/target *inputs* were valid enough to
    // produce a complete plan — independent of the (always-failing) terminal
    // real-writer gate.
    let inputs_valid =
        flags_valid && patch_plan_present && diff_report_present && files_present && target_is_rpf;
    let status = if inputs_valid {
        RpfWritePlanStatus::Planned
    } else {
        RpfWritePlanStatus::Blocked
    };

    Ok(RpfWritePlan {
        safe_to_write,
        status,
        input_bundle_path: bundle_dir.to_string_lossy().to_string(),
        target_archive_path: target_archive_path.to_string_lossy().to_string(),
        target_archive_type,
        files_to_replace,
        backup_plan,
        restore_plan,
        hash_verification_required: true,
        manual_confirmation_required: true,
        safety_gates: gates,
        blocked,
        summary: RpfWriteSummary {
            target_count,
            gate_count,
            passed_gate_count,
            blocking_gate_count,
            blocked_count,
        },
        modifies_rpf: false,
        real_writer_implemented: false,
    })
}
