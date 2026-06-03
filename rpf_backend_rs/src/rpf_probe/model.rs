use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RpfProbeStatus {
    Probed,
    Blocked,
}

/// File-level metadata gathered without parsing the archive internals.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfProbeFileInfo {
    pub exists: bool,
    pub is_file: bool,
    pub extension_valid: bool,
    pub size_bytes: Option<u64>,
    pub hash_algorithm: String,
    pub sha256: Option<String>,
}

/// A named capability and whether it is currently available.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfProbeCapability {
    pub name: String,
    pub available: bool,
    pub detail: String,
}

/// Result of checking whether an external tool appears available on PATH.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfProbeToolCheck {
    pub tool: String,
    pub found: bool,
    /// How the tool was looked up (e.g. `path_lookup`), informational only.
    pub method: String,
    pub detail: String,
}

/// A condition that prevented a complete probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfProbeBlockedItem {
    pub path: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfProbeSummary {
    pub can_read_metadata: bool,
    pub tools_checked: usize,
    pub tools_found: usize,
    pub blocked_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfProbeReport {
    pub status: RpfProbeStatus,

    /// Target archive path (only read — never modified).
    pub target_archive_path: String,

    pub exists: bool,
    pub is_file: bool,
    pub extension_valid: bool,
    pub size_bytes: Option<u64>,
    pub hash_algorithm: String,
    pub sha256: Option<String>,

    pub file_info: RpfProbeFileInfo,

    pub can_read_metadata: bool,
    /// RPF internals are not parsed in this milestone.
    pub can_parse_rpf: bool,
    /// No write path exists in this milestone.
    pub can_write_rpf: bool,
    /// No native RPF writer exists.
    pub native_writer_implemented: bool,

    pub external_tools: Vec<RpfProbeToolCheck>,
    pub capabilities: Vec<RpfProbeCapability>,

    pub blocked: Vec<RpfProbeBlockedItem>,
    pub summary: RpfProbeSummary,

    /// This command never modifies the target archive.
    pub modifies_target_archive: bool,
}
