use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RpfBackupStatus {
    BackedUp,
    Blocked,
}

/// Describes the copied backup file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfBackupFile {
    /// Backup filename within the backup directory.
    pub file_name: String,
    /// Absolute path of the backup copy.
    pub backup_path: String,
    pub size_bytes: u64,
}

/// Result of comparing the original archive hash against the backup hash.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfBackupHashVerification {
    pub algorithm: String,
    pub original_hash: String,
    pub backup_hash: String,
    pub hash_verified: bool,
}

/// A condition that prevented a verified backup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfBackupBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfBackupSummary {
    pub backup_created: bool,
    pub hash_verified: bool,
    pub blocked_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfBackupReport {
    pub status: RpfBackupStatus,

    /// Intended target archive (never modified).
    pub target_archive_path: String,
    /// Backup directory used.
    pub backup_dir: String,
    /// Absolute path of the backup file; `None` when blocked before copying.
    pub backup_file_path: Option<String>,

    pub original_size_bytes: Option<u64>,
    pub backup_size_bytes: Option<u64>,

    pub hash_algorithm: String,
    pub original_hash: Option<String>,
    pub backup_hash: Option<String>,
    pub hash_verified: bool,

    /// `true` only when a verified backup was created and all checks passed.
    pub safe_for_future_write: bool,

    pub blocked: Vec<RpfBackupBlockedItem>,
    pub summary: RpfBackupSummary,

    // Mirrored safety facts: this milestone never writes the archive.
    pub modifies_target_archive: bool,
    pub real_writer_implemented: bool,
}
