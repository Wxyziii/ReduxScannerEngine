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

/// Build a blocked report (nothing was copied).
fn blocked_report(
    target_archive_path: &Path,
    backup_dir: &Path,
    block_type: &str,
    reason: String,
) -> RpfBackupReport {
    RpfBackupReport {
        status: RpfBackupStatus::Blocked,
        target_archive_path: target_archive_path.to_string_lossy().to_string(),
        backup_dir: backup_dir.to_string_lossy().to_string(),
        backup_file_path: None,
        original_size_bytes: None,
        backup_size_bytes: None,
        hash_algorithm: HASH_ALGORITHM.to_string(),
        original_hash: None,
        backup_hash: None,
        hash_verified: false,
        safe_for_future_write: false,
        blocked: vec![RpfBackupBlockedItem {
            path: target_archive_path.to_string_lossy().to_string(),
            reason,
            block_type: block_type.to_string(),
        }],
        summary: RpfBackupSummary {
            backup_created: false,
            hash_verified: false,
            blocked_count: 1,
        },
        modifies_target_archive: false,
        real_writer_implemented: false,
    }
}

/// Deterministic backup filename: `<original-name>.<hash-prefix>.backup`.
fn backup_file_name(target_archive_path: &Path, original_hash: &str) -> String {
    let base = target_archive_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive.rpf".to_string());
    let prefix: String = original_hash.chars().take(12).collect();
    format!("{}.{}.backup", base, prefix)
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Copy a target `.rpf` archive into `backup_dir` and verify the copy by
/// SHA-256. The original archive is only read — never modified or written.
///
/// `safe_for_future_write` is set `true` only when the copy succeeds and the
/// original and backup hashes match. No RPF writing is performed or enabled.
pub fn backup_rpf_archive(
    target_archive_path: &Path,
    backup_dir: &Path,
) -> Result<RpfBackupReport, String> {
    // 1. Target must exist.
    if !target_archive_path.exists() {
        return Ok(blocked_report(
            target_archive_path,
            backup_dir,
            "missing_target",
            format!(
                "Target archive does not exist: {}",
                target_archive_path.display()
            ),
        ));
    }

    // 2. Target must be a file (not a directory).
    if !target_archive_path.is_file() {
        return Ok(blocked_report(
            target_archive_path,
            backup_dir,
            "target_not_a_file",
            format!(
                "Target archive is not a file: {}",
                target_archive_path.display()
            ),
        ));
    }

    // 3. Target must have a .rpf extension.
    if !has_rpf_extension(target_archive_path) {
        return Ok(blocked_report(
            target_archive_path,
            backup_dir,
            "non_rpf_target",
            format!(
                "Target archive path must end with .rpf: {}",
                target_archive_path.display()
            ),
        ));
    }

    // 4. Read original bytes (read-only) and hash.
    let original_bytes = fs::read(target_archive_path).map_err(|e| {
        format!(
            "Failed to read target archive {}: {}",
            target_archive_path.display(),
            e
        )
    })?;
    let original_size = original_bytes.len() as u64;
    let original_hash = sha256_hex(&original_bytes);

    // 5. Create the backup directory if needed.
    fs::create_dir_all(backup_dir).map_err(|e| {
        format!(
            "Failed to create backup dir {}: {}",
            backup_dir.display(),
            e
        )
    })?;

    // 6. Write the backup copy (into backup_dir only — never the target).
    let file_name = backup_file_name(target_archive_path, &original_hash);
    let backup_path = backup_dir.join(&file_name);
    fs::write(&backup_path, &original_bytes)
        .map_err(|e| format!("Failed to write backup {}: {}", backup_path.display(), e))?;

    // 7. Read the backup back and hash it for verification.
    let backup_bytes = fs::read(&backup_path)
        .map_err(|e| format!("Failed to read backup {}: {}", backup_path.display(), e))?;
    let backup_size = backup_bytes.len() as u64;
    let backup_hash = sha256_hex(&backup_bytes);

    let hash_verified = original_hash == backup_hash && original_size == backup_size;
    let safe_for_future_write = hash_verified;

    let status = if hash_verified {
        RpfBackupStatus::BackedUp
    } else {
        RpfBackupStatus::Blocked
    };

    let mut blocked: Vec<RpfBackupBlockedItem> = Vec::new();
    if !hash_verified {
        blocked.push(RpfBackupBlockedItem {
            path: backup_path.to_string_lossy().to_string(),
            reason: "Backup hash does not match original hash".to_string(),
            block_type: "hash_mismatch".to_string(),
        });
    }

    Ok(RpfBackupReport {
        status,
        target_archive_path: target_archive_path.to_string_lossy().to_string(),
        backup_dir: backup_dir.to_string_lossy().to_string(),
        backup_file_path: Some(backup_path.to_string_lossy().to_string()),
        original_size_bytes: Some(original_size),
        backup_size_bytes: Some(backup_size),
        hash_algorithm: HASH_ALGORITHM.to_string(),
        original_hash: Some(original_hash),
        backup_hash: Some(backup_hash),
        hash_verified,
        safe_for_future_write,
        blocked,
        summary: RpfBackupSummary {
            backup_created: true,
            hash_verified,
            blocked_count: if hash_verified { 0 } else { 1 },
        },
        modifies_target_archive: false,
        real_writer_implemented: false,
    })
}
