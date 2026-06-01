use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfEntryManifestStatus {
    Built,
    Blocked,
}

/// A single planned archive entry replacement, sourced from a bundle file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfEntryManifestEntry {
    /// Normalized path inside the archive (forward slashes, no traversal).
    pub archive_relative_path: String,
    /// Path of the providing file relative to the bundle dir (e.g. `files/...`).
    pub bundle_file_relative_path: String,
    /// Absolute path of the providing bundle file.
    pub bundle_file_absolute_path: String,
    pub extension: String,
    pub size_bytes: u64,
    pub hash_algorithm: String,
    pub sha256: Option<String>,
    /// Where the replacement content comes from. Always `bundle/files`.
    pub replacement_source: String,
    /// A future writer would replace an existing archive entry at this path.
    pub would_replace_existing_entry: bool,
    pub safe_path: bool,
    /// The planned operation. Always `replace_file_planned` in this milestone.
    pub operation_kind: String,
}

/// A condition that prevented a complete/safe manifest.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfEntryManifestBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

/// A non-fatal observation worth surfacing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfEntryManifestWarning {
    pub path: String,
    pub message: String,
    pub warning_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfEntryManifestSummary {
    pub entry_count: usize,
    pub duplicate_count: usize,
    pub unsafe_path_count: usize,
    pub blocked_count: usize,
    pub warning_count: usize,
}

/// The future-writer input manifest itself (the durable schema).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfEntryManifest {
    pub manifest_version: String,
    pub bundle_dir: String,
    pub target_rpf: Option<String>,
    pub entries: Vec<RpfEntryManifestEntry>,

    // ── Safety flags (all conservative this milestone) ──────────────────────
    pub modifies_rpf: bool,
    pub native_parser_used: bool,
    pub native_writer_used: bool,
    pub external_tool_used: bool,
    pub ready_for_write: bool,
}

/// The full report wrapping the manifest plus diagnostics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfEntryManifestReport {
    pub status: RpfEntryManifestStatus,
    pub bundle_dir: String,
    pub target_rpf: Option<String>,

    pub manifest: RpfEntryManifest,

    pub blocked: Vec<RpfEntryManifestBlockedItem>,
    pub warnings: Vec<RpfEntryManifestWarning>,
    pub summary: RpfEntryManifestSummary,

    // ── Mirrored safety flags ───────────────────────────────────────────────
    pub modifies_rpf: bool,
    pub native_parser_used: bool,
    pub native_writer_used: bool,
    pub external_tool_used: bool,
    pub ready_for_write: bool,
}
