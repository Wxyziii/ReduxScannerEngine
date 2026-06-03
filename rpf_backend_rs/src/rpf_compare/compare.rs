use super::model::*;
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

/// Role label for an archive slot, used in block/difference messages.
fn empty_file_info(
    path: &Path,
    exists: bool,
    is_file: bool,
    extension_valid: bool,
) -> RpfCompareFileInfo {
    RpfCompareFileInfo {
        path: path.to_string_lossy().to_string(),
        exists,
        is_file,
        extension_valid,
        size_bytes: None,
        hash_algorithm: HASH_ALGORITHM.to_string(),
        sha256: None,
    }
}

/// Validate a single archive path and read its size + SHA-256 (read-only).
/// On success returns a populated `RpfCompareFileInfo`. On failure returns the
/// best-known partial info plus a blocked item describing the problem.
fn read_archive(
    path: &Path,
    role: &str,
) -> Result<RpfCompareFileInfo, (RpfCompareFileInfo, RpfCompareBlockedItem)> {
    let exists = path.exists();
    if !exists {
        return Err((
            empty_file_info(path, false, false, has_rpf_extension(path)),
            RpfCompareBlockedItem {
                path: path.to_string_lossy().to_string(),
                reason: format!("{} archive does not exist: {}", role, path.display()),
                block_type: format!("missing_{}_target", role),
            },
        ));
    }

    let is_file = path.is_file();
    if !is_file {
        return Err((
            empty_file_info(path, true, false, has_rpf_extension(path)),
            RpfCompareBlockedItem {
                path: path.to_string_lossy().to_string(),
                reason: format!("{} archive is not a file: {}", role, path.display()),
                block_type: format!("{}_target_not_a_file", role),
            },
        ));
    }

    let extension_valid = has_rpf_extension(path);
    if !extension_valid {
        return Err((
            empty_file_info(path, true, true, false),
            RpfCompareBlockedItem {
                path: path.to_string_lossy().to_string(),
                reason: format!(
                    "{} archive path must end with .rpf: {}",
                    role,
                    path.display()
                ),
                block_type: format!("non_rpf_{}_target", role),
            },
        ));
    }

    // Read bytes (read-only) → size + SHA-256. We do NOT parse RPF internals.
    let bytes = fs::read(path).map_err(|e| {
        (
            empty_file_info(path, true, true, true),
            RpfCompareBlockedItem {
                path: path.to_string_lossy().to_string(),
                reason: format!("Failed to read {} archive {}: {}", role, path.display(), e),
                block_type: format!("{}_read_failed", role),
            },
        )
    })?;
    let size = bytes.len() as u64;
    let hash = sha256_hex(&bytes);

    Ok(RpfCompareFileInfo {
        path: path.to_string_lossy().to_string(),
        exists: true,
        is_file: true,
        extension_valid: true,
        size_bytes: Some(size),
        hash_algorithm: HASH_ALGORITHM.to_string(),
        sha256: Some(hash),
    })
}

fn build_report(
    status: RpfCompareStatus,
    clean_path: &Path,
    modded_path: &Path,
    clean_info: RpfCompareFileInfo,
    modded_info: RpfCompareFileInfo,
    size_differs: bool,
    hash_differs: bool,
    differences: Vec<RpfCompareDifference>,
    blocked: Vec<RpfCompareBlockedItem>,
) -> RpfCompareReport {
    let archives_differ = size_differs || hash_differs;
    RpfCompareReport {
        status,
        clean_archive_path: clean_path.to_string_lossy().to_string(),
        modded_archive_path: modded_path.to_string_lossy().to_string(),
        clean_size_bytes: clean_info.size_bytes,
        modded_size_bytes: modded_info.size_bytes,
        hash_algorithm: HASH_ALGORITHM.to_string(),
        clean_sha256: clean_info.sha256.clone(),
        modded_sha256: modded_info.sha256.clone(),
        clean_file_info: clean_info,
        modded_file_info: modded_info,
        size_differs,
        hash_differs,
        archives_differ,
        differences: differences.clone(),
        can_compare_internals: false,
        native_parser_implemented: false,
        summary: RpfCompareSummary {
            archives_differ,
            size_differs,
            hash_differs,
            difference_count: differences.len(),
            blocked_count: blocked.len(),
        },
        blocked,
        modifies_clean_archive: false,
        modifies_modded_archive: false,
    }
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Read-only comparison of two `.rpf` archives by external file metadata and
/// SHA-256 only. Neither archive is parsed or modified.
pub fn compare_rpf_archives(
    clean_archive_path: &Path,
    modded_archive_path: &Path,
) -> Result<RpfCompareReport, String> {
    let mut blocked: Vec<RpfCompareBlockedItem> = Vec::new();

    let clean_info = match read_archive(clean_archive_path, "clean") {
        Ok(info) => info,
        Err((info, block)) => {
            blocked.push(block);
            info
        }
    };
    let modded_info = match read_archive(modded_archive_path, "modded") {
        Ok(info) => info,
        Err((info, block)) => {
            blocked.push(block);
            info
        }
    };

    if !blocked.is_empty() {
        return Ok(build_report(
            RpfCompareStatus::Blocked,
            clean_archive_path,
            modded_archive_path,
            clean_info,
            modded_info,
            false,
            false,
            Vec::new(),
            blocked,
        ));
    }

    // Both archives read successfully — compare size and hash.
    let size_differs = clean_info.size_bytes != modded_info.size_bytes;
    let hash_differs = clean_info.sha256 != modded_info.sha256;

    let mut differences: Vec<RpfCompareDifference> = Vec::new();
    if size_differs {
        differences.push(RpfCompareDifference {
            kind: "size".to_string(),
            clean_value: clean_info
                .size_bytes
                .map(|v| v.to_string())
                .unwrap_or_default(),
            modded_value: modded_info
                .size_bytes
                .map(|v| v.to_string())
                .unwrap_or_default(),
            detail: "File sizes differ.".to_string(),
        });
    }
    if hash_differs {
        differences.push(RpfCompareDifference {
            kind: "hash".to_string(),
            clean_value: clean_info.sha256.clone().unwrap_or_default(),
            modded_value: modded_info.sha256.clone().unwrap_or_default(),
            detail: "SHA-256 hashes differ.".to_string(),
        });
    }

    Ok(build_report(
        RpfCompareStatus::Compared,
        clean_archive_path,
        modded_archive_path,
        clean_info,
        modded_info,
        size_differs,
        hash_differs,
        differences,
        Vec::new(),
    ))
}
