use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RpfCompareStatus {
    Compared,
    Blocked,
}

/// File-level metadata gathered for a single archive without parsing internals.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfCompareFileInfo {
    pub path: String,
    pub exists: bool,
    pub is_file: bool,
    pub extension_valid: bool,
    pub size_bytes: Option<u64>,
    pub hash_algorithm: String,
    pub sha256: Option<String>,
}

/// A single observed difference between the two archives.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfCompareDifference {
    /// What differs, e.g. `size` or `hash`.
    pub kind: String,
    pub clean_value: String,
    pub modded_value: String,
    pub detail: String,
}

/// A condition that prevented a complete comparison.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfCompareBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfCompareSummary {
    pub archives_differ: bool,
    pub size_differs: bool,
    pub hash_differs: bool,
    pub difference_count: usize,
    pub blocked_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfCompareReport {
    pub status: RpfCompareStatus,

    /// Input archive paths (only read — never modified).
    pub clean_archive_path: String,
    pub modded_archive_path: String,

    pub clean_file_info: RpfCompareFileInfo,
    pub modded_file_info: RpfCompareFileInfo,

    pub clean_size_bytes: Option<u64>,
    pub modded_size_bytes: Option<u64>,

    pub hash_algorithm: String,
    pub clean_sha256: Option<String>,
    pub modded_sha256: Option<String>,

    pub size_differs: bool,
    pub hash_differs: bool,
    pub archives_differ: bool,

    pub differences: Vec<RpfCompareDifference>,

    /// RPF internals are not parsed in this milestone.
    pub can_compare_internals: bool,
    /// No native RPF parser exists.
    pub native_parser_implemented: bool,

    pub blocked: Vec<RpfCompareBlockedItem>,
    pub summary: RpfCompareSummary,

    /// This command never modifies either input archive.
    pub modifies_clean_archive: bool,
    pub modifies_modded_archive: bool,
}
