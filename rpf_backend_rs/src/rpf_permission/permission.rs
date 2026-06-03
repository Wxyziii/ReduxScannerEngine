use super::model::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// The exact phrase the user must supply via `--confirm`.
pub const EXPECTED_CONFIRMATION_PHRASE: &str =
    "I understand this is planning-only and does not write the RPF";

const TOKEN_VERSION: &str = "1";

// ── Read-only deserialize views ──────────────────────────────────────────────

/// Minimal view of a `write-readiness` report. Unknown fields are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadinessReportView {
    #[serde(default)]
    ready_to_write: bool,
    #[serde(default)]
    target_rpf: Option<String>,
}

/// Minimal view of an `rpf-entry-manifest` report. Unknown fields are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntryManifestReportView {
    #[serde(default)]
    ready_for_write: bool,
    #[serde(default)]
    target_rpf: Option<String>,
}

/// Minimal view of a `backup-rpf` report. Unknown fields are ignored.
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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn gate(
    name: &str,
    passed: bool,
    severity: RpfWriterPermissionSeverity,
    message: &str,
) -> RpfWriterPermissionGate {
    RpfWriterPermissionGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Best-effort path equality (canonicalize when possible; fall back to literal).
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Outcome of reading one optional input report.
struct ReportOutcome {
    provided: bool,
    present: bool,
    valid: bool,
    detail: String,
}

impl ReportOutcome {
    /// A provided-but-absent/invalid report is a problem; an unprovided one is OK.
    fn ok_or_absent(&self) -> bool {
        !self.provided || self.valid
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("unreadable: {}", e))
}

fn read_readiness(path: Option<&Path>, target_rpf: &Path) -> ReportOutcome {
    let Some(path) = path else {
        return ReportOutcome {
            provided: false,
            present: false,
            valid: false,
            detail: "No readiness report supplied (optional).".to_string(),
        };
    };
    if !path.is_file() {
        return ReportOutcome {
            provided: true,
            present: false,
            valid: false,
            detail: format!("Readiness report not found: {}", path.display()),
        };
    }
    let content = match read_text(path) {
        Ok(c) => c,
        Err(e) => {
            return ReportOutcome {
                provided: true,
                present: false,
                valid: false,
                detail: format!("Readiness report {}", e),
            }
        }
    };
    let view: ReadinessReportView = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return ReportOutcome {
                provided: true,
                present: true,
                valid: false,
                detail: format!("Readiness report could not be parsed: {}", e),
            }
        }
    };
    let target_matches = match view.target_rpf.as_deref() {
        Some(p) => paths_equal(Path::new(p), target_rpf),
        None => true,
    };
    // In this milestone the readiness report MUST say readyToWrite=false.
    let valid = !view.ready_to_write && target_matches;
    let detail = if view.ready_to_write {
        "Readiness report unexpectedly reports readyToWrite=true; rejected.".to_string()
    } else if !target_matches {
        "Readiness report target path does not match --target-rpf.".to_string()
    } else {
        "Readiness report present; readyToWrite=false; target matches.".to_string()
    };
    ReportOutcome {
        provided: true,
        present: true,
        valid,
        detail,
    }
}

fn read_entry_manifest(path: Option<&Path>, target_rpf: &Path) -> ReportOutcome {
    let Some(path) = path else {
        return ReportOutcome {
            provided: false,
            present: false,
            valid: false,
            detail: "No entry manifest report supplied (optional).".to_string(),
        };
    };
    if !path.is_file() {
        return ReportOutcome {
            provided: true,
            present: false,
            valid: false,
            detail: format!("Entry manifest report not found: {}", path.display()),
        };
    }
    let content = match read_text(path) {
        Ok(c) => c,
        Err(e) => {
            return ReportOutcome {
                provided: true,
                present: false,
                valid: false,
                detail: format!("Entry manifest report {}", e),
            }
        }
    };
    let view: EntryManifestReportView = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return ReportOutcome {
                provided: true,
                present: true,
                valid: false,
                detail: format!("Entry manifest report could not be parsed: {}", e),
            }
        }
    };
    let target_matches = match view.target_rpf.as_deref() {
        Some(p) => paths_equal(Path::new(p), target_rpf),
        None => true,
    };
    let valid = !view.ready_for_write && target_matches;
    let detail = if view.ready_for_write {
        "Entry manifest unexpectedly reports readyForWrite=true; rejected.".to_string()
    } else if !target_matches {
        "Entry manifest target path does not match --target-rpf.".to_string()
    } else {
        "Entry manifest present; readyForWrite=false; target matches.".to_string()
    };
    ReportOutcome {
        provided: true,
        present: true,
        valid,
        detail,
    }
}

fn read_backup(path: Option<&Path>, target_rpf: &Path) -> ReportOutcome {
    let Some(path) = path else {
        return ReportOutcome {
            provided: false,
            present: false,
            valid: false,
            detail: "No backup report supplied (optional).".to_string(),
        };
    };
    if !path.is_file() {
        return ReportOutcome {
            provided: true,
            present: false,
            valid: false,
            detail: format!("Backup report not found: {}", path.display()),
        };
    }
    let content = match read_text(path) {
        Ok(c) => c,
        Err(e) => {
            return ReportOutcome {
                provided: true,
                present: false,
                valid: false,
                detail: format!("Backup report {}", e),
            }
        }
    };
    let view: BackupReportView = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return ReportOutcome {
                provided: true,
                present: true,
                valid: false,
                detail: format!("Backup report could not be parsed: {}", e),
            }
        }
    };
    let target_matches = match view.target_archive_path.as_deref() {
        Some(p) => paths_equal(Path::new(p), target_rpf),
        None => true,
    };
    let verified = view.hash_verified
        && view.safe_for_future_write
        && view.status.as_deref() != Some("blocked");
    let valid = verified && target_matches;
    let detail = if !verified {
        "Backup report present but not hash-verified/safe for future write.".to_string()
    } else if !target_matches {
        "Backup report target path does not match --target-rpf.".to_string()
    } else {
        "Backup report present, hash-verified, and safe for future write.".to_string()
    };
    ReportOutcome {
        provided: true,
        present: true,
        valid,
        detail,
    }
}

fn token_id(bundle_dir: &Path, target_rpf: &Path) -> String {
    // Deterministic id derived from the inputs — no randomness, no clock.
    let mut hasher = Sha256::new();
    hasher.update(b"redux_writer_permission_token\0");
    hasher.update(bundle_dir.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(target_rpf.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(EXPECTED_CONFIRMATION_PHRASE.as_bytes());
    let hex = hex::encode(hasher.finalize());
    format!("wpt_{}", &hex[..32])
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Build a read-only writer-permission report. Models the manual confirmation
/// required before any future RPF write. NEVER opens or modifies the target
/// archive, NEVER modifies the bundle, NEVER creates backups, and NEVER
/// executes external tools. `writer_allowed` is always `false`.
pub fn build_writer_permission_report(
    bundle_dir: &Path,
    target_rpf: &Path,
    readiness_report_path: Option<&Path>,
    entry_manifest_report_path: Option<&Path>,
    backup_report_path: Option<&Path>,
    confirmation_phrase: Option<&str>,
) -> Result<RpfWriterPermissionReport, String> {
    // ── Input presence / shape ──────────────────────────────────────────────
    let bundle_dir_present = bundle_dir.is_dir();
    let target_present = target_rpf.exists();
    let ext_valid = target_rpf
        .extension()
        .map(|e| e.eq_ignore_ascii_case("rpf"))
        .unwrap_or(false);

    // ── Optional input reports ──────────────────────────────────────────────
    let readiness = read_readiness(readiness_report_path, target_rpf);
    let entry_manifest = read_entry_manifest(entry_manifest_report_path, target_rpf);
    let backup = read_backup(backup_report_path, target_rpf);

    // ── Confirmation phrase ─────────────────────────────────────────────────
    let confirmation_phrase_provided = confirmation_phrase.map(|p| !p.is_empty()).unwrap_or(false);
    let confirmation_phrase_matched = confirmation_phrase == Some(EXPECTED_CONFIRMATION_PHRASE);

    // ── Adapter capability (active adapter is safe-mode only) ────────────────
    let adapter = NullRpfAdapter::new();
    let adapter_can_write = adapter.capabilities().can_write_archive;

    // ── Gates ────────────────────────────────────────────────────────────────
    let mut gates: Vec<RpfWriterPermissionGate> = Vec::new();
    let mut blocked: Vec<RpfWriterPermissionBlockedItem> = Vec::new();

    gates.push(gate(
        "bundle_dir_present",
        bundle_dir_present,
        if bundle_dir_present {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        if bundle_dir_present {
            "Bundle directory exists."
        } else {
            "Bundle directory does not exist."
        },
    ));
    if !bundle_dir_present {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "bundle".to_string(),
            reason: "The supplied --bundle-dir does not exist.".to_string(),
            block_type: "bundle_dir_missing".to_string(),
        });
    }

    gates.push(gate(
        "target_rpf_present",
        target_present,
        if target_present {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        if target_present {
            "Target RPF path exists."
        } else {
            "Target RPF path does not exist."
        },
    ));
    if !target_present {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "target".to_string(),
            reason: "The supplied --target-rpf does not exist.".to_string(),
            block_type: "target_rpf_missing".to_string(),
        });
    }

    gates.push(gate(
        "target_rpf_extension_valid",
        ext_valid,
        if ext_valid {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        if ext_valid {
            "Target has a .rpf extension."
        } else {
            "Target does not have a .rpf extension."
        },
    ));
    if !ext_valid {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "target".to_string(),
            reason: "The supplied --target-rpf is not a .rpf file.".to_string(),
            block_type: "target_not_rpf".to_string(),
        });
    }

    // Readiness report gates.
    let readiness_present_ok = !readiness.provided || readiness.present;
    gates.push(gate(
        "readiness_report_present_or_missing",
        readiness_present_ok,
        if readiness_present_ok {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        &readiness.detail,
    ));
    gates.push(gate(
        "readiness_report_valid_if_present",
        readiness.ok_or_absent(),
        if readiness.ok_or_absent() {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        &readiness.detail,
    ));
    if readiness.provided && !readiness.valid {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "readiness".to_string(),
            reason: readiness.detail.clone(),
            block_type: "readiness_report_invalid".to_string(),
        });
    }

    // Entry manifest report gates.
    let entry_present_ok = !entry_manifest.provided || entry_manifest.present;
    gates.push(gate(
        "entry_manifest_report_present_or_missing",
        entry_present_ok,
        if entry_present_ok {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        &entry_manifest.detail,
    ));
    gates.push(gate(
        "entry_manifest_valid_if_present",
        entry_manifest.ok_or_absent(),
        if entry_manifest.ok_or_absent() {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        &entry_manifest.detail,
    ));
    if entry_manifest.provided && !entry_manifest.valid {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "entry_manifest".to_string(),
            reason: entry_manifest.detail.clone(),
            block_type: "entry_manifest_report_invalid".to_string(),
        });
    }

    // Backup report gates.
    let backup_present_ok = !backup.provided || backup.present;
    gates.push(gate(
        "backup_report_present_or_missing",
        backup_present_ok,
        if backup_present_ok {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        &backup.detail,
    ));
    gates.push(gate(
        "backup_report_hash_verified_if_present",
        backup.ok_or_absent(),
        if backup.ok_or_absent() {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        &backup.detail,
    ));
    if backup.provided && !backup.valid {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "backup".to_string(),
            reason: backup.detail.clone(),
            block_type: "backup_report_invalid".to_string(),
        });
    }

    // Confirmation gates.
    gates.push(gate(
        "confirmation_phrase_provided",
        confirmation_phrase_provided,
        if confirmation_phrase_provided {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        if confirmation_phrase_provided {
            "A confirmation phrase was supplied via --confirm."
        } else {
            "No confirmation phrase supplied; --confirm is required."
        },
    ));
    if !confirmation_phrase_provided {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "confirmation".to_string(),
            reason: "No confirmation phrase was supplied.".to_string(),
            block_type: "confirmation_phrase_missing".to_string(),
        });
    }
    gates.push(gate(
        "confirmation_phrase_matched",
        confirmation_phrase_matched,
        if confirmation_phrase_matched {
            RpfWriterPermissionSeverity::Info
        } else {
            RpfWriterPermissionSeverity::Blocking
        },
        if confirmation_phrase_matched {
            "Confirmation phrase matches exactly."
        } else {
            "Confirmation phrase does not match the required exact phrase."
        },
    ));
    if confirmation_phrase_provided && !confirmation_phrase_matched {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "confirmation".to_string(),
            reason: "Confirmation phrase does not match the required exact phrase.".to_string(),
            block_type: "confirmation_phrase_mismatch".to_string(),
        });
    }

    // ── Terminal blockers (always present this milestone) ────────────────────
    gates.push(gate(
        "real_rpf_writer_implemented",
        false,
        RpfWriterPermissionSeverity::Blocking,
        "The real RPF writer is not implemented.",
    ));
    blocked.push(RpfWriterPermissionBlockedItem {
        component: "writer".to_string(),
        reason: "The real RPF writer is not implemented.".to_string(),
        block_type: "real_rpf_writer_not_implemented".to_string(),
    });

    gates.push(gate(
        "native_rpf_parser_implemented",
        false,
        RpfWriterPermissionSeverity::Blocking,
        "Native RPF parsing is not implemented.",
    ));
    blocked.push(RpfWriterPermissionBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });

    gates.push(gate(
        "adapter_supports_write",
        adapter_can_write,
        RpfWriterPermissionSeverity::Blocking,
        "The active adapter (NullRpfAdapter) cannot write archives.",
    ));
    if !adapter_can_write {
        blocked.push(RpfWriterPermissionBlockedItem {
            component: "adapter".to_string(),
            reason: "The active adapter is safe-mode only and cannot write archives.".to_string(),
            block_type: "active_adapter_cannot_write".to_string(),
        });
    }

    // writer_permission_allowed is ALWAYS false in this milestone.
    gates.push(gate(
        "writer_permission_allowed",
        false,
        RpfWriterPermissionSeverity::Blocking,
        "Writing is not authorized: the writer/parser/adapter do not yet support it.",
    ));

    // ── Token issuance ───────────────────────────────────────────────────────
    let inputs_ok = bundle_dir_present
        && target_present
        && ext_valid
        && readiness.ok_or_absent()
        && entry_manifest.ok_or_absent()
        && backup.ok_or_absent();
    let token_issued = inputs_ok && confirmation_phrase_matched;

    let permission_token = if token_issued {
        Some(RpfWriterPermissionToken {
            token_version: TOKEN_VERSION.to_string(),
            token_id: token_id(bundle_dir, target_rpf),
            bundle_dir: bundle_dir.to_string_lossy().to_string(),
            target_rpf: target_rpf.to_string_lossy().to_string(),
            confirmed_target_rpf: true,
            confirmed_backup_required: true,
            confirmed_restore_required: true,
            confirmed_hash_verification_required: true,
            confirmed_manual_action: confirmation_phrase_matched,
            ready_to_write_at_creation: false,
            writer_allowed: false,
            created_from_reports: RpfWriterPermissionSources {
                readiness_report: readiness_report_path.map(|p| p.to_string_lossy().to_string()),
                entry_manifest_report: entry_manifest_report_path
                    .map(|p| p.to_string_lossy().to_string()),
                backup_report: backup_report_path.map(|p| p.to_string_lossy().to_string()),
            },
            modifies_rpf: false,
            external_tool_used: false,
            native_writer_used: false,
        })
    } else {
        None
    };

    // ── Summary / status ─────────────────────────────────────────────────────
    let total_gates = gates.len();
    let passed_gates = gates.iter().filter(|g| g.passed).count();
    let blocking_gates = gates
        .iter()
        .filter(|g| g.severity == RpfWriterPermissionSeverity::Blocking && !g.passed)
        .count();
    let blocked_count = blocked.len();

    let status = if token_issued {
        RpfWriterPermissionStatus::TokenIssued
    } else {
        RpfWriterPermissionStatus::Blocked
    };

    Ok(RpfWriterPermissionReport {
        status,
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        target_rpf: target_rpf.to_string_lossy().to_string(),
        readiness_report_path: readiness_report_path.map(|p| p.to_string_lossy().to_string()),
        entry_manifest_report_path: entry_manifest_report_path
            .map(|p| p.to_string_lossy().to_string()),
        backup_report_path: backup_report_path.map(|p| p.to_string_lossy().to_string()),
        confirmation_phrase_provided,
        expected_confirmation_phrase: EXPECTED_CONFIRMATION_PHRASE.to_string(),
        confirmation_phrase_matched,
        permission_token,
        writer_allowed: false,
        gates,
        blocked,
        summary: RpfWriterPermissionSummary {
            total_gates,
            passed_gates,
            blocking_gates,
            blocked_count,
            token_issued,
            writer_allowed: false,
        },
        modifies_target_archive: false,
        real_writer_implemented: false,
        native_parser_implemented: false,
    })
}
