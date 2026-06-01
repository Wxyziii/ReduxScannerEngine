use super::model::*;
use super::tools::detect_external_tools;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const HASH_ALGORITHM: &str = "SHA-256";

// ── Helpers ─────────────────────────────────────────────────────────────────

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn has_rpf_extension(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("rpf"))
        .unwrap_or(false)
}

/// Capability list — fixed for this milestone (no parser, no writer).
fn current_capabilities() -> Vec<RpfProbeCapability> {
    vec![
        RpfProbeCapability {
            name: "read_metadata".to_string(),
            available: true,
            detail: "File size and SHA-256 can be read without parsing internals.".to_string(),
        },
        RpfProbeCapability {
            name: "parse_rpf".to_string(),
            available: false,
            detail: "RPF internal parsing is not performed in this milestone.".to_string(),
        },
        RpfProbeCapability {
            name: "write_rpf".to_string(),
            available: false,
            detail: "No RPF write path exists; real writing is not implemented.".to_string(),
        },
        RpfProbeCapability {
            name: "native_writer".to_string(),
            available: false,
            detail: "No native RPF writer is implemented.".to_string(),
        },
    ]
}

/// Build a blocked report (no hash computed; metadata reflects what was known).
fn blocked_report(
    target_archive_path: &Path,
    exists: bool,
    is_file: bool,
    extension_valid: bool,
    block_type: &str,
    reason: String,
) -> RpfProbeReport {
    let external_tools = detect_external_tools();
    let tools_checked = external_tools.len();
    let tools_found = external_tools.iter().filter(|t| t.found).count();

    RpfProbeReport {
        status: RpfProbeStatus::Blocked,
        target_archive_path: target_archive_path.to_string_lossy().to_string(),
        exists,
        is_file,
        extension_valid,
        size_bytes: None,
        hash_algorithm: HASH_ALGORITHM.to_string(),
        sha256: None,
        file_info: RpfProbeFileInfo {
            exists,
            is_file,
            extension_valid,
            size_bytes: None,
            hash_algorithm: HASH_ALGORITHM.to_string(),
            sha256: None,
        },
        can_read_metadata: false,
        can_parse_rpf: false,
        can_write_rpf: false,
        native_writer_implemented: false,
        external_tools,
        capabilities: current_capabilities(),
        blocked: vec![RpfProbeBlockedItem {
            path: target_archive_path.to_string_lossy().to_string(),
            reason,
            block_type: block_type.to_string(),
        }],
        summary: RpfProbeSummary {
            can_read_metadata: false,
            tools_checked,
            tools_found,
            blocked_count: 1,
        },
        modifies_target_archive: false,
    }
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Read-only probe of a target `.rpf` archive: file metadata + SHA-256, plus
/// informational external-tool detection. The archive is never parsed or
/// modified; no backup is created here.
pub fn probe_rpf_archive(target_archive_path: &Path) -> Result<RpfProbeReport, String> {
    let exists = target_archive_path.exists();
    if !exists {
        return Ok(blocked_report(
            target_archive_path,
            false,
            false,
            has_rpf_extension(target_archive_path),
            "missing_target",
            format!(
                "Target archive does not exist: {}",
                target_archive_path.display()
            ),
        ));
    }

    let is_file = target_archive_path.is_file();
    if !is_file {
        return Ok(blocked_report(
            target_archive_path,
            true,
            false,
            has_rpf_extension(target_archive_path),
            "target_not_a_file",
            format!(
                "Target archive is not a file: {}",
                target_archive_path.display()
            ),
        ));
    }

    let extension_valid = has_rpf_extension(target_archive_path);
    if !extension_valid {
        return Ok(blocked_report(
            target_archive_path,
            true,
            true,
            false,
            "non_rpf_target",
            format!(
                "Target archive path must end with .rpf: {}",
                target_archive_path.display()
            ),
        ));
    }

    // Read bytes (read-only) → size + SHA-256. We do NOT parse RPF internals.
    let bytes = fs::read(target_archive_path).map_err(|e| {
        format!(
            "Failed to read target archive {}: {}",
            target_archive_path.display(),
            e
        )
    })?;
    let size = bytes.len() as u64;
    let hash = sha256_hex(&bytes);

    let external_tools = detect_external_tools();
    let tools_checked = external_tools.len();
    let tools_found = external_tools.iter().filter(|t| t.found).count();

    Ok(RpfProbeReport {
        status: RpfProbeStatus::Probed,
        target_archive_path: target_archive_path.to_string_lossy().to_string(),
        exists: true,
        is_file: true,
        extension_valid: true,
        size_bytes: Some(size),
        hash_algorithm: HASH_ALGORITHM.to_string(),
        sha256: Some(hash.clone()),
        file_info: RpfProbeFileInfo {
            exists: true,
            is_file: true,
            extension_valid: true,
            size_bytes: Some(size),
            hash_algorithm: HASH_ALGORITHM.to_string(),
            sha256: Some(hash),
        },
        can_read_metadata: true,
        can_parse_rpf: false,
        can_write_rpf: false,
        native_writer_implemented: false,
        external_tools,
        capabilities: current_capabilities(),
        blocked: Vec::new(),
        summary: RpfProbeSummary {
            can_read_metadata: true,
            tools_checked,
            tools_found,
            blocked_count: 0,
        },
        modifies_target_archive: false,
    })
}
