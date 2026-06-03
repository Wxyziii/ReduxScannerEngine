use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RpfWritePlanStatus {
    /// A plan was produced. Writing is never permitted in this milestone, so this
    /// only means the bundle/target inputs were understood well enough to plan.
    Planned,
    /// The inputs were invalid (missing manifest, bad target, etc.).
    Blocked,
}

/// Severity for a single safety gate.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GateSeverity {
    Info,
    Warning,
    Blocking,
}

/// A single safety gate that must be satisfied before any real RPF write.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriteSafetyGate {
    pub gate: String,
    pub passed: bool,
    pub severity: GateSeverity,
    pub message: String,
}

/// A file that a future writer would replace inside the target archive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriteTarget {
    /// Relative path within the archive (mirrors the bundle `files/` layout).
    pub relative_path: String,
    /// Absolute path of the patched file inside the bundle.
    pub bundle_file_path: String,
    pub size_bytes: u64,
    pub extension: String,
    /// SHA-256 of the bundle file, if recorded in the manifest.
    pub sha256: Option<String>,
}

/// What a future writer would back up before touching the archive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriteBackupPlan {
    pub required: bool,
    /// Human description of the backup strategy.
    pub strategy: String,
    /// Suggested backup destination label (no file is created in this milestone).
    pub suggested_backup_path: String,
}

/// How a future writer would restore the archive if a write failed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriteRestorePlan {
    pub required: bool,
    pub strategy: String,
}

/// An input problem that prevents a complete plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriteBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriteSummary {
    pub target_count: usize,
    pub gate_count: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub blocked_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWritePlan {
    /// Always `false` in this milestone — no real writer exists yet.
    pub safe_to_write: bool,
    pub status: RpfWritePlanStatus,

    /// Input bundle directory.
    pub input_bundle_path: String,
    /// Intended target archive path (never opened or modified).
    pub target_archive_path: String,
    /// Label/type of the target archive, e.g. `update.rpf`.
    pub target_archive_type: String,

    /// Files that a future writer would replace.
    pub files_to_replace: Vec<RpfWriteTarget>,

    pub backup_plan: RpfWriteBackupPlan,
    pub restore_plan: RpfWriteRestorePlan,
    /// Whether SHA-256 verification of written entries would be required.
    pub hash_verification_required: bool,
    /// Whether explicit human confirmation would be required before writing.
    pub manual_confirmation_required: bool,

    pub safety_gates: Vec<RpfWriteSafetyGate>,
    pub blocked: Vec<RpfWriteBlockedItem>,
    pub summary: RpfWriteSummary,

    // Mirrored safety facts for easy consumption.
    pub modifies_rpf: bool,
    pub real_writer_implemented: bool,
}
