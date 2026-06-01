use serde::Serialize;

/// What kind of adapter is backing RPF operations.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfAdapterKind {
    /// Safe placeholder adapter — no parsing or writing.
    Null,
    /// A future in-process native RPF implementation.
    Native,
    /// A future adapter that drives an external tool (e.g. OpenIV/CodeWalker).
    ExternalTool,
}

/// Outcome status for a planned or executed adapter operation.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfAdapterStatus {
    /// Operation is supported and could proceed (read-only operations only here).
    Ready,
    /// Operation is recognized but refused by safety policy.
    Blocked,
    /// Operation has no implementation behind it yet.
    NotImplemented,
}

/// The set of operations an adapter may eventually support.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfAdapterOperation {
    ProbeMetadata,
    ListEntries,
    ExtractFile,
    ReplaceFile,
    WriteArchive,
}

impl RpfAdapterOperation {
    /// Stable string identifier used in reports and block items.
    pub fn as_str(&self) -> &'static str {
        match self {
            RpfAdapterOperation::ProbeMetadata => "probe_metadata",
            RpfAdapterOperation::ListEntries => "list_entries",
            RpfAdapterOperation::ExtractFile => "extract_file",
            RpfAdapterOperation::ReplaceFile => "replace_file",
            RpfAdapterOperation::WriteArchive => "write_archive",
        }
    }

    /// Parse an operation identifier (snake_case). Returns `None` if unknown.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "probe_metadata" | "probe-metadata" => Some(RpfAdapterOperation::ProbeMetadata),
            "list_entries" | "list-entries" => Some(RpfAdapterOperation::ListEntries),
            "extract_file" | "extract-file" => Some(RpfAdapterOperation::ExtractFile),
            "replace_file" | "replace-file" => Some(RpfAdapterOperation::ReplaceFile),
            "write_archive" | "write-archive" => Some(RpfAdapterOperation::WriteArchive),
            _ => None,
        }
    }

    /// Every operation the contract defines, in a stable order.
    pub fn all() -> &'static [RpfAdapterOperation] {
        &[
            RpfAdapterOperation::ProbeMetadata,
            RpfAdapterOperation::ListEntries,
            RpfAdapterOperation::ExtractFile,
            RpfAdapterOperation::ReplaceFile,
            RpfAdapterOperation::WriteArchive,
        ]
    }
}

/// Declared capabilities of an adapter. In this milestone everything that could
/// parse or modify an archive is `false`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfAdapterCapabilities {
    pub can_probe_metadata: bool,
    pub can_list_entries: bool,
    pub can_extract_files: bool,
    pub can_replace_files: bool,
    pub can_write_archive: bool,
    pub requires_external_tool: bool,
    pub native_parser: bool,
    pub native_writer: bool,
    pub safe_mode_only: bool,
}

/// A refused/unsupported operation, with a machine-readable block type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfAdapterBlockedItem {
    pub operation: String,
    pub reason: String,
    pub block_type: String,
}

/// The result of planning a single operation (no side effects).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfAdapterOperationPlan {
    pub operation: RpfAdapterOperation,
    pub supported: bool,
    pub status: RpfAdapterStatus,
    pub detail: String,
    pub blocked: Vec<RpfAdapterBlockedItem>,
    /// Whether executing this operation would modify the archive. Always false here.
    pub modifies_archive: bool,
}

/// The result of "executing" a single operation. In this milestone nothing is
/// ever actually executed and `modified_archive` is always false.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfAdapterOperationResult {
    pub operation: RpfAdapterOperation,
    pub executed: bool,
    pub status: RpfAdapterStatus,
    pub detail: String,
    pub blocked: Vec<RpfAdapterBlockedItem>,
    /// Whether the archive was modified. Always false in this milestone.
    pub modified_archive: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfAdapterSummary {
    pub operation_count: usize,
    pub supported_operation_count: usize,
    pub blocked_operation_count: usize,
    pub safe_mode_only: bool,
}

/// Full capability report emitted by `rpf-adapter-info`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfAdapterInfoReport {
    pub adapter_name: String,
    pub adapter_kind: RpfAdapterKind,
    pub capabilities: RpfAdapterCapabilities,
    pub operation_plans: Vec<RpfAdapterOperationPlan>,
    pub blocked: Vec<RpfAdapterBlockedItem>,
    pub summary: RpfAdapterSummary,
    /// No native adapter exists yet.
    pub native_adapter_implemented: bool,
    pub note: String,
    /// This command never modifies any archive.
    pub modifies_archive: bool,
}
