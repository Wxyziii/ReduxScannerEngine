use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfWriterPermissionStatus {
    /// All input checks passed and the confirmation phrase matched, so a
    /// planning token was issued. Writing is still NOT allowed.
    TokenIssued,
    /// Inputs were invalid or the confirmation phrase was missing/mismatched,
    /// so no token could be issued.
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RpfWriterPermissionSeverity {
    Info,
    Warning,
    Blocking,
}

/// A single permission gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriterPermissionGate {
    pub name: String,
    pub passed: bool,
    pub severity: RpfWriterPermissionSeverity,
    pub message: String,
}

/// A reason writing is not (and cannot yet be) authorized.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriterPermissionBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// Which source reports a token was created from (paths only — no embedding).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriterPermissionSources {
    pub readiness_report: Option<String>,
    pub entry_manifest_report: Option<String>,
    pub backup_report: Option<String>,
}

/// The planning permission token. Its mere existence does NOT permit writing:
/// `writer_allowed` is always `false` in this milestone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriterPermissionToken {
    pub token_version: String,
    pub token_id: String,
    pub bundle_dir: String,
    pub target_rpf: String,

    /// The user explicitly supplied (and the phrase confirmed) this target path.
    pub confirmed_target_rpf: bool,
    pub confirmed_backup_required: bool,
    pub confirmed_restore_required: bool,
    pub confirmed_hash_verification_required: bool,
    pub confirmed_manual_action: bool,

    /// Always `false`: a token never authorizes writing in this milestone.
    pub ready_to_write_at_creation: bool,
    pub writer_allowed: bool,

    pub created_from_reports: RpfWriterPermissionSources,

    // ── Safety flags (all conservative this milestone) ──────────────────────
    pub modifies_rpf: bool,
    pub external_tool_used: bool,
    pub native_writer_used: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriterPermissionSummary {
    pub total_gates: usize,
    pub passed_gates: usize,
    pub blocking_gates: usize,
    pub blocked_count: usize,
    pub token_issued: bool,
    pub writer_allowed: bool,
}

/// The read-only writer-permission report. Models the final manual confirmation
/// object required before any future RPF writing. `writer_allowed` is always
/// `false` because the writer is not implemented.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpfWriterPermissionReport {
    pub status: RpfWriterPermissionStatus,

    pub bundle_dir: String,
    pub target_rpf: String,
    pub readiness_report_path: Option<String>,
    pub entry_manifest_report_path: Option<String>,
    pub backup_report_path: Option<String>,

    pub confirmation_phrase_provided: bool,
    pub expected_confirmation_phrase: String,
    pub confirmation_phrase_matched: bool,

    pub permission_token: Option<RpfWriterPermissionToken>,

    /// Always `false` in this milestone.
    pub writer_allowed: bool,

    pub gates: Vec<RpfWriterPermissionGate>,
    pub blocked: Vec<RpfWriterPermissionBlockedItem>,
    pub summary: RpfWriterPermissionSummary,

    // ── Mirrored safety facts ───────────────────────────────────────────────
    pub modifies_target_archive: bool,
    pub real_writer_implemented: bool,
    pub native_parser_implemented: bool,
}
