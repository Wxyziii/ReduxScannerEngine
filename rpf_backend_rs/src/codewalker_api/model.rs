use serde::Serialize;

/// Overall outcome of a read-only CodeWalker.API detection pass.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerApiDetectionStatus {
    /// A CodeWalker.API server answered the read-only status endpoint.
    Detected,
    /// The host answered HTTP but did not look like CodeWalker.API.
    Reachable,
    /// Nothing answered at the base URL.
    Offline,
    /// The provided base URL was not usable.
    InvalidBaseUrl,
}

/// Severity of a detection safety gate.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerApiSeverity {
    Info,
    Warning,
    Blocking,
}

/// Result of a single read-only HTTP GET probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiEndpointCheck {
    pub name: String,
    pub url: String,
    /// The HTTP method used. Always `GET` — read-only.
    pub method: String,
    pub checked: bool,
    pub available: bool,
    pub http_status: Option<u16>,
    pub detail: Option<String>,
}

/// A capability the detector observed or declined. Write capabilities stay false.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiCapability {
    pub name: String,
    pub available: bool,
    pub description: String,
}

/// A reason writing remains blocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A safety gate covering this milestone's read-only contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiSafetyGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiSummary {
    pub endpoints_checked: usize,
    pub endpoints_available: usize,
    pub blocked_count: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub reachable: bool,
    pub codewalker_api_detected: bool,
    pub writer_allowed: bool,
}

/// Read-only CodeWalker.API detection report. Performs only safe HTTP GET status
/// checks against the base URL. Never calls write/replace/import endpoints, never
/// executes CodeWalker as a process, never opens or modifies an RPF archive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiDetectionReport {
    pub status: CodeWalkerApiDetectionStatus,

    pub base_url: String,
    pub normalized_base_url: String,
    pub default_base_url_used: bool,

    pub reachable: bool,
    pub tcp_reachable: bool,

    pub service_status_checked: bool,
    pub service_status_available: bool,
    pub service_status_http_status: Option<u16>,

    pub root_checked: bool,
    pub root_available: bool,
    pub root_http_status: Option<u16>,

    pub codewalker_api_detected: bool,
    /// `false` unless the status endpoint clearly reports readiness.
    pub codewalker_ready: bool,

    pub can_detect: bool,
    pub can_call_readonly_status: bool,
    pub can_replace_file: bool,
    pub can_import_file: bool,
    pub can_write_archive: bool,

    pub write_endpoints_checked: bool,
    pub write_endpoints_called: bool,
    pub replace_endpoint_called: bool,
    pub import_endpoint_called: bool,
    pub external_tool_executed: bool,
    pub modifies_archive: bool,
    pub writer_allowed: bool,

    pub active_adapter_name: String,

    pub endpoint_checks: Vec<CodeWalkerApiEndpointCheck>,
    pub capabilities: Vec<CodeWalkerApiCapability>,
    pub blocked_items: Vec<CodeWalkerApiBlockedItem>,
    pub safety_gates: Vec<CodeWalkerApiSafetyGate>,
    pub summary: CodeWalkerApiSummary,
}

/// Overall readiness verdict for future search/replace planning.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerApiReadinessStatus {
    /// Service reachable and status clearly reports readiness.
    Ready,
    /// Service reachable but readiness could not be confirmed.
    ReachableNotReady,
    /// Nothing answered at the base URL.
    Offline,
    /// The provided base URL was not usable.
    InvalidBaseUrl,
}

/// Parsed-from-service-status facts. Every field is best-effort and optional —
/// an unexpected JSON shape never fails the probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiServiceStatusInfo {
    pub json_parse_success: bool,
    pub ready: Option<bool>,
    pub status_text: Option<String>,
    pub services_ready: Option<bool>,
    pub gta_path_detected: bool,
    pub gta_path: Option<String>,
    pub reload_version: Option<String>,
}

/// A readiness safety gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiReadinessGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

/// A non-blocking advisory observed while probing readiness.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiReadinessWarning {
    pub code: String,
    pub message: String,
}

/// A reason readiness/writing remains blocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiReadinessBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiReadinessSummary {
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub warning_count: usize,
    pub blocked_count: usize,
    pub reachable: bool,
    pub ready_for_search: bool,
    pub ready_for_replace: bool,
    pub writer_allowed: bool,
}

/// Read-only CodeWalker.API readiness report. Builds on detection and adds a
/// tolerant parse of `/api/service-status`. Uses only HTTP GET. Never calls
/// replace/import/reload-services/set-config or any mutation/POST endpoint,
/// never executes CodeWalker, never opens or modifies an RPF archive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerApiReadinessReport {
    pub status: CodeWalkerApiReadinessStatus,

    pub base_url: String,
    pub normalized_base_url: String,

    pub detection_report: CodeWalkerApiDetectionReport,

    pub service_status_raw: Option<String>,
    pub service_status_json_parse_success: bool,
    pub service_status_http_status: Option<u16>,
    pub service_status_info: CodeWalkerApiServiceStatusInfo,

    pub gta_path_detected: bool,
    pub gta_path: Option<String>,
    pub reload_version: Option<String>,
    pub services_ready: Option<bool>,

    pub codewalker_api_reachable: bool,
    pub codewalker_api_ready_for_search: bool,
    pub codewalker_api_ready_for_replace: bool,

    pub can_call_search_later: bool,
    pub can_call_replace_later: bool,
    pub can_write_archive: bool,

    pub write_endpoints_called: bool,
    pub replace_endpoint_called: bool,
    pub import_endpoint_called: bool,
    pub reload_services_called: bool,
    pub set_config_called: bool,
    pub mutation_endpoints_called: bool,
    pub post_requests_used: bool,
    pub external_tool_executed: bool,
    pub modifies_archive: bool,
    pub writer_allowed: bool,

    pub active_adapter_name: String,

    pub gates: Vec<CodeWalkerApiReadinessGate>,
    pub warnings: Vec<CodeWalkerApiReadinessWarning>,
    pub blocked_items: Vec<CodeWalkerApiReadinessBlockedItem>,
    pub summary: CodeWalkerApiReadinessSummary,
}

/// Overall outcome of a search + target-resolution pass.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerSearchResolveStatus {
    /// Manifest loaded and searches were run against a reachable API.
    Completed,
    /// API was not reachable; all targets unresolved.
    Offline,
    /// API reachable but not ready for search.
    NotReady,
    /// The entry manifest could not be read/loaded, or the base URL was invalid.
    InvalidInput,
}

/// How strongly a candidate matches the requested archive-relative path.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerSearchConfidence {
    Exact,
    Suffix,
    FilenameOnly,
    None,
}

/// A single search result candidate returned by CodeWalker.API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchCandidate {
    pub raw_path: String,
    pub normalized_path: String,
    pub filename: String,
    pub matches_filename: bool,
    pub matches_archive_relative_path_suffix: bool,
    pub confidence: CodeWalkerSearchConfidence,
    pub selected: bool,
}

/// The actual HTTP GET issued for one target.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchRequest {
    pub method: String,
    pub url: String,
    pub requested_filename: String,
    pub http_status: Option<u16>,
    pub succeeded: bool,
    pub detail: Option<String>,
}

/// A manifest entry to resolve, plus its search outcome.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchTarget {
    pub archive_relative_path: String,
    pub requested_filename: String,
    pub search_url_path: String,
    pub candidates: Vec<CodeWalkerSearchCandidate>,
    pub exact_match_found: bool,
    pub suffix_match_found: bool,
    pub ambiguous: bool,
    pub resolved: bool,
    pub match_type: CodeWalkerSearchConfidence,
    pub selected_candidate: Option<String>,
    pub reason: String,
}

/// A resolved target (selected candidate present).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerResolvedTarget {
    pub archive_relative_path: String,
    pub selected_candidate: String,
    pub match_type: CodeWalkerSearchConfidence,
}

/// An unresolved target with the reason it could not resolve.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerUnresolvedTarget {
    pub archive_relative_path: String,
    pub requested_filename: String,
    pub reason: String,
    pub candidate_count: usize,
}

/// A search safety gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchSafetyGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

/// A reason resolution/writing remains blocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A non-fatal advisory observed while resolving.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchSummary {
    pub target_count: usize,
    pub resolved_count: usize,
    pub unresolved_count: usize,
    pub ambiguous_count: usize,
    pub search_request_count: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub warning_count: usize,
    pub blocked_count: usize,
    pub reachable: bool,
    pub writer_allowed: bool,
}

/// Read-only CodeWalker search + target-resolution report. Maps RPF entry
/// manifest entries to CodeWalker search results via GET `/api/search-file`.
/// Uses only HTTP GET. Never calls replace/import/reload-services/set-config or
/// any mutation/POST endpoint, never executes CodeWalker, never opens or modifies
/// an RPF archive. `canWriteArchive` and `writerAllowed` stay `false`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerSearchResolveReport {
    pub status: CodeWalkerSearchResolveStatus,

    pub base_url: String,
    pub normalized_base_url: String,
    pub entry_manifest_report_path: String,

    pub readiness_report_path: Option<String>,
    pub readiness_checked: bool,

    pub codewalker_api_reachable: bool,
    pub codewalker_api_ready_for_search: bool,
    pub search_endpoint_used: String,

    pub targets: Vec<CodeWalkerSearchTarget>,
    pub resolved_targets: Vec<CodeWalkerResolvedTarget>,
    pub unresolved_targets: Vec<CodeWalkerUnresolvedTarget>,
    pub ambiguous_targets: Vec<CodeWalkerUnresolvedTarget>,
    pub search_requests: Vec<CodeWalkerSearchRequest>,

    pub get_requests_only: bool,
    pub post_requests_used: bool,
    pub replace_endpoint_called: bool,
    pub import_endpoint_called: bool,
    pub reload_services_called: bool,
    pub set_config_called: bool,
    pub mutation_endpoints_called: bool,
    pub external_tool_executed: bool,
    pub modifies_archive: bool,
    pub writer_allowed: bool,
    pub can_write_archive: bool,

    pub active_adapter_name: String,

    pub blocked_items: Vec<CodeWalkerSearchBlockedItem>,
    pub warnings: Vec<CodeWalkerSearchWarning>,
    pub safety_gates: Vec<CodeWalkerSearchSafetyGate>,
    pub summary: CodeWalkerSearchSummary,
}

// ── T0.6.3 dry replace plan ─────────────────────────────────────────────────

/// Overall outcome of a dry replace plan pass.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerDryReplacePlanStatus {
    /// Every manifest entry produced a valid planned replace payload.
    Planned,
    /// Some entries are valid; others are blocked.
    Partial,
    /// No entry could be planned (all blocked).
    Blocked,
    /// A required input (bundle dir / manifest / resolve report) was unusable.
    InvalidInput,
}

/// The bundle file backing a planned replace, plus its hash facts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplaceSourceFile {
    pub bundle_file_relative_path: String,
    pub bundle_file_absolute_path: String,
    pub bundle_file_exists: bool,
    pub bundle_file_size_bytes: u64,
    pub bundle_file_sha256: Option<String>,
    pub manifest_sha256: Option<String>,
    pub hash_matches_manifest: bool,
}

/// The CodeWalker-resolved archive target for a planned replace.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplaceResolvedTarget {
    pub archive_relative_path: String,
    pub codewalker_resolved_path: Option<String>,
    pub match_type: Option<String>,
    pub resolved: bool,
    pub ambiguous: bool,
}

/// A conservative model of the future `/api/replace-file` request body. This is
/// NEVER sent anywhere in this milestone — it is structured planning only.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplacePayload {
    /// Always `/api/replace-file`.
    pub endpoint: String,
    /// Always `POST` (modelled, never issued).
    pub method: String,
    pub rpf_path: Option<String>,
    pub archive_path: Option<String>,
    pub source_file_path: String,
    pub archive_relative_path: String,
    /// Always `true`: this payload describes a dry-run plan only.
    pub dry_run_only: bool,
}

/// A single planned (or blocked) replace, combining a manifest entry, the
/// CodeWalker resolved target, and the providing bundle file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplaceItem {
    pub archive_relative_path: String,
    pub codewalker_resolved_path: Option<String>,
    pub bundle_file_relative_path: String,
    pub bundle_file_absolute_path: String,
    pub bundle_file_exists: bool,
    pub bundle_file_size_bytes: u64,
    pub bundle_file_sha256: Option<String>,
    pub manifest_sha256: Option<String>,
    pub hash_matches_manifest: bool,
    pub exact_or_suffix_match_type: Option<String>,
    pub source_file: CodeWalkerDryReplaceSourceFile,
    pub resolved_target: CodeWalkerDryReplaceResolvedTarget,
    /// Present only when the item is valid for a future replace.
    pub planned_payload: Option<CodeWalkerDryReplacePayload>,
    pub valid_for_future_replace: bool,
    pub blocked_reason: Option<String>,
}

/// A reason an item (or the plan) cannot proceed to a future replace.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplaceBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A non-fatal advisory observed while planning.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplaceWarning {
    pub code: String,
    pub message: String,
}

/// A dry replace safety gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplaceSafetyGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplaceSummary {
    pub item_count: usize,
    pub valid_item_count: usize,
    pub blocked_item_count: usize,
    pub planned_request_count: usize,
    pub resolved_target_count: usize,
    pub hash_match_count: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub warning_count: usize,
    pub blocked_count: usize,
    pub ready_for_execution: bool,
    pub writer_allowed: bool,
}

/// Read-only CodeWalker dry replace plan. Combines the T0.5.7 entry manifest,
/// the T0.6.2 resolve report, the providing bundle files, and an optional
/// T0.5.8 writer-permission report into a set of MODELLED `/api/replace-file`
/// payloads. It reads only local report/bundle files. It issues NO HTTP request
/// of any kind, never uses POST, never calls replace/import/reload-services/
/// set-config or any mutation endpoint, never executes CodeWalker or any external
/// tool, and never opens or modifies an RPF archive. `readyForExecution`,
/// `writerAllowed`, and `codewalkerExecutionAllowed` all stay `false`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerDryReplacePlanReport {
    pub status: CodeWalkerDryReplacePlanStatus,

    pub bundle_dir: String,
    pub entry_manifest_report_path: String,
    pub resolve_report_path: String,
    pub permission_report_path: Option<String>,

    pub selected_writer_route: String,
    pub active_adapter_name: String,

    pub dry_run_only: bool,
    pub ready_for_execution: bool,
    pub writer_allowed: bool,
    pub codewalker_execution_allowed: bool,
    pub can_write_archive: bool,

    pub planned_endpoint: String,
    pub planned_http_method: String,

    pub items: Vec<CodeWalkerDryReplaceItem>,
    pub planned_requests: Vec<CodeWalkerDryReplacePayload>,
    pub blocked_items: Vec<CodeWalkerDryReplaceBlockedItem>,
    pub warnings: Vec<CodeWalkerDryReplaceWarning>,
    pub safety_gates: Vec<CodeWalkerDryReplaceSafetyGate>,
    pub summary: CodeWalkerDryReplaceSummary,

    // ── Mirrored safety facts (all conservative this milestone) ─────────────
    pub post_requests_sent: bool,
    pub get_requests_sent: bool,
    pub replace_endpoint_called: bool,
    pub import_endpoint_called: bool,
    pub reload_services_called: bool,
    pub set_config_called: bool,
    pub mutation_endpoints_called: bool,
    pub external_tool_executed: bool,
    pub modifies_archive: bool,
    pub real_writer_implemented: bool,
    pub native_parser_implemented: bool,
}

// ── T0.6.4 copied-test-archive execution gate ───────────────────────────────

/// Overall verdict of an execution-gate pass. Even when `Eligible`, NO execution
/// happens in this milestone.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerExecutionGateStatus {
    /// Every strict copied-test-archive gate passed. A future attempt would be
    /// eligible — but is still not performed or allowed now.
    Eligible,
    /// At least one strict gate failed. A future attempt is not eligible.
    Blocked,
    /// A required input report/file was unusable; eligibility cannot be decided.
    InvalidInput,
}

/// How the target archive path was classified for test-execution eligibility.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerTargetArchiveClassification {
    /// A copied test archive explicitly confirmed as a test copy, not in an
    /// original game install path. The only class eligible for future execution.
    CopiedTestArchive,
    /// The path looks like an original game install — always blocked.
    OriginalGameArchiveSuspected,
    /// Not obviously a game path, but not confirmed as a test copy either.
    UnknownArchive,
    /// The target file does not exist.
    Missing,
    /// The target exists but does not have a `.rpf` extension.
    InvalidExtension,
}

/// Load/parse status of one input report this gate reads.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerExecutionInputReportStatus {
    /// Present, parsed, and satisfied the gate's expectations.
    Valid,
    /// Present and parsed, but did not satisfy expectations.
    Invalid,
    /// Present but could not be parsed.
    Unparsable,
    /// File was not found.
    Missing,
}

/// A single execution-gate check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerExecutionGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

/// A reason a future execution attempt is not eligible.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerExecutionGateBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A non-fatal advisory observed while gating.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerExecutionGateWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerExecutionGateSummary {
    pub total_gates: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub warning_count: usize,
    pub blocked_count: usize,
    pub strict_gates_all_passed: bool,
    pub codewalker_execution_eligible: bool,
    pub codewalker_execution_allowed_now: bool,
    pub codewalker_execution_performed: bool,
    pub writer_allowed: bool,
    pub modifies_archive: bool,
}

/// Read-only CodeWalker copied-test-archive execution gate. Decides whether a
/// FUTURE CodeWalker replace attempt against the given target archive would even
/// be eligible. It reads only local report/fixture files. It issues NO HTTP
/// request of any kind, never uses POST, never calls replace/import/
/// reload-services/set-config or any mutation endpoint, never executes CodeWalker
/// or any external tool, and never opens or modifies an RPF archive. Even when
/// `codewalkerExecutionEligible` is `true`, `codewalkerExecutionAllowedNow`,
/// `codewalkerExecutionPerformed`, `writerAllowed`, and `modifiesArchive` all
/// stay `false` — no execution happens in this milestone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerExecutionGateReport {
    pub status: CodeWalkerExecutionGateStatus,

    // ── Target archive facts ────────────────────────────────────────────────
    pub target_rpf: String,
    pub target_rpf_exists: bool,
    pub target_rpf_extension_valid: bool,
    pub target_archive_classification: CodeWalkerTargetArchiveClassification,
    pub target_marked_as_test_copy: bool,
    pub target_path_allowed_for_test_execution: bool,

    // ── Input report paths ──────────────────────────────────────────────────
    pub dry_replace_plan_path: String,
    pub permission_report_path: String,
    pub readiness_report_path: String,
    pub entry_manifest_report_path: String,
    pub backup_report_path: String,

    // ── Input report validity ───────────────────────────────────────────────
    pub dry_replace_plan_status: CodeWalkerExecutionInputReportStatus,
    pub permission_report_status: CodeWalkerExecutionInputReportStatus,
    pub readiness_report_status: CodeWalkerExecutionInputReportStatus,
    pub entry_manifest_report_status: CodeWalkerExecutionInputReportStatus,
    pub backup_report_status: CodeWalkerExecutionInputReportStatus,

    pub dry_replace_plan_valid: bool,
    pub permission_report_valid: bool,
    pub readiness_report_valid: bool,
    pub entry_manifest_report_valid: bool,
    pub backup_report_valid: bool,

    // ── Extracted facts ─────────────────────────────────────────────────────
    pub backup_hash_verified: bool,
    pub permission_token_present: bool,
    pub dry_plan_has_planned_requests: bool,
    /// Expected `false` from T0.6.3 but still valid as a dry plan.
    pub dry_plan_ready_for_execution: bool,

    // ── Verdict ──────────────────────────────────────────────────────────────
    pub codewalker_execution_eligible: bool,
    pub codewalker_execution_performed: bool,
    pub codewalker_execution_allowed_now: bool,
    pub writer_allowed: bool,

    // ── Adapter / safety mirror (all conservative this milestone) ───────────
    pub active_adapter_name: String,
    pub null_adapter_active: bool,
    pub replace_endpoint_called: bool,
    pub import_endpoint_called: bool,
    pub reload_services_called: bool,
    pub set_config_called: bool,
    pub post_requests_sent: bool,
    pub http_requests_sent: bool,
    pub external_tool_executed: bool,
    pub modifies_archive: bool,
    pub real_writer_implemented: bool,
    pub native_parser_implemented: bool,

    pub gates: Vec<CodeWalkerExecutionGate>,
    pub warnings: Vec<CodeWalkerExecutionGateWarning>,
    pub blocked_items: Vec<CodeWalkerExecutionGateBlockedItem>,
    pub summary: CodeWalkerExecutionGateSummary,
}

// ── T0.6.5 controlled replace apply on a copied test archive ─────────────────

/// Overall outcome of a controlled replace-apply pass.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerReplaceApplyStatus {
    /// Every gate passed and every replace request returned success.
    Executed,
    /// Requests were sent; some succeeded, some failed.
    PartiallyExecuted,
    /// Requests were sent but every one failed (transport or non-2xx).
    Failed,
    /// A strict gate failed; NO HTTP request was sent.
    Blocked,
    /// A required input report was missing/unparsable; NO HTTP request was sent.
    InvalidInput,
}

/// Whether the local target file's hash changed across the apply.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerReplaceTargetHashChange {
    Changed,
    Unchanged,
    /// Target not accessible locally before and/or after; cannot compare.
    Unknown,
}

/// The MODELLED-then-SENT replace request for one planned item. The payload is
/// conservative and fully visible; the exact CodeWalker.API shape may evolve.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplyRequest {
    /// Always `POST`.
    pub method: String,
    /// Full URL dialed: normalized base URL + replace endpoint.
    pub url: String,
    /// Always `/api/replace-file`.
    pub endpoint: String,
    pub rpf_path: Option<String>,
    pub archive_path: Option<String>,
    pub source_file_path: String,
    pub archive_relative_path: String,
    /// Execution marker actually sent (`false` — this is a real apply, not dry).
    pub dry_run_only: bool,
    /// The exact JSON body sent, for auditability.
    pub request_body_json: String,
}

/// The response captured for one replace request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplyResponse {
    pub sent: bool,
    pub http_status: Option<u16>,
    pub succeeded: bool,
    pub response_body_summary: Option<String>,
    pub error: Option<String>,
}

/// One planned item's full request/response result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplyItemResult {
    pub archive_relative_path: String,
    pub codewalker_resolved_path: Option<String>,
    pub source_file_path: String,
    pub request: CodeWalkerReplaceApplyRequest,
    pub response: CodeWalkerReplaceApplyResponse,
}

/// A replace-apply safety gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplySafetyGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

/// A reason the apply was blocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplyBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A non-fatal advisory observed while applying.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplyWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplySummary {
    pub total_gates: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub warning_count: usize,
    pub blocked_count: usize,
    pub replace_request_count: usize,
    pub successful_replace_count: usize,
    pub failed_replace_count: usize,
    pub codewalker_execution_performed: bool,
    pub modifies_archive: bool,
    /// Global writer remains disabled regardless of this scoped execution.
    pub writer_allowed: bool,
}

/// Read-only-by-default, scoped CodeWalker replace executor. This is the FIRST
/// milestone that may issue a CodeWalker replace HTTP request — but only when the
/// T0.6.4 execution gate is eligible, the target is a copied test archive,
/// `--execute` is given, and the exact confirmation phrase matches. It sends ONLY
/// `POST /api/replace-file`. It never calls import/reload-services/set-config or
/// the search endpoint, never executes CodeWalker as a process, never executes an
/// external tool, never parses RPF internals, and never auto-rolls-back. The
/// global `writerAllowed` stays `false` and the active adapter stays
/// `NullRpfAdapter`. On any blocking gate failure, NO HTTP request is sent.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerReplaceApplyReport {
    pub status: CodeWalkerReplaceApplyStatus,

    pub base_url: String,
    pub normalized_base_url: String,
    pub execution_gate_report_path: String,
    pub dry_replace_plan_path: String,
    pub target_rpf: String,

    // ── Inputs / authorization ──────────────────────────────────────────────
    pub execute_requested: bool,
    pub confirmation_phrase_provided: bool,
    pub confirmation_phrase_matched: bool,
    pub expected_confirmation_phrase: String,
    pub execution_gate_eligible: bool,
    pub copied_test_archive_confirmed: bool,

    pub selected_writer_route: String,
    pub active_adapter_name: String,
    pub null_adapter_active: bool,
    pub replace_endpoint: String,

    // ── Execution facts ─────────────────────────────────────────────────────
    pub replace_requests_sent: bool,
    pub replace_request_count: usize,
    pub successful_replace_count: usize,
    pub failed_replace_count: usize,
    pub codewalker_execution_performed: bool,
    pub codewalker_execution_allowed_now: bool,
    /// Scoped to this command's gated execution. Global `writer_allowed` is false.
    pub execution_scoped_writer_allowed: bool,
    pub writer_allowed: bool,
    pub modifies_archive: bool,

    // ── Hash audit ──────────────────────────────────────────────────────────
    pub original_target_sha256: Option<String>,
    pub post_execution_target_sha256: Option<String>,
    pub target_hash_changed: CodeWalkerReplaceTargetHashChange,

    // ── Endpoint-isolation mirror (all conservative) ────────────────────────
    pub import_endpoint_called: bool,
    pub reload_services_called: bool,
    pub set_config_called: bool,
    pub search_endpoint_called: bool,
    pub external_tool_executed: bool,
    pub native_parser_used: bool,
    pub native_writer_used: bool,
    pub rollback_performed: bool,

    pub gates: Vec<CodeWalkerReplaceApplySafetyGate>,
    pub warnings: Vec<CodeWalkerReplaceApplyWarning>,
    pub blocked_items: Vec<CodeWalkerReplaceApplyBlockedItem>,
    pub item_results: Vec<CodeWalkerReplaceApplyItemResult>,
    pub summary: CodeWalkerReplaceApplySummary,
}

// ── T0.6.6 post-write verification + rollback plan ───────────────────────────

/// Overall outcome of a post-write verification pass.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerPostWriteVerifyStatus {
    /// Target read, hashes compared, and a verification result was determined.
    Verified,
    /// A required input report/target was unusable; verification incomplete.
    InvalidInput,
}

/// The interpreted result of comparing a replace attempt against target hashes.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerPostWriteResult {
    /// No replace request was sent and the target is unchanged.
    NoExecutionNoChange,
    /// A replace request failed and the target is unchanged.
    ExecutionFailedNoChange,
    /// A replace request failed yet the target changed — suspicious.
    ExecutionFailedButTargetChangedSuspicious,
    /// A replace request succeeded and the target changed — expected.
    ExecutionSucceededTargetChanged,
    /// A replace request succeeded yet the target is unchanged — suspicious.
    ExecutionSucceededButTargetUnchangedSuspicious,
    /// State could not be classified from the available facts.
    Unknown,
}

/// Status of the generated rollback plan.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerRollbackPlanStatus {
    /// A usable rollback plan was built from a verified backup.
    Ready,
    /// No usable rollback plan could be built (missing/unverified backup).
    Unavailable,
}

/// A planned (never executed) rollback to the verified backup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerRollbackPlan {
    pub rollback_plan_status: CodeWalkerRollbackPlanStatus,
    pub target_rpf: String,
    pub backup_file_path: Option<String>,
    pub backup_sha256: Option<String>,
    pub target_current_sha256: Option<String>,
    /// Always `copy_backup_over_target` — the planned (future) restore method.
    pub restore_method_planned: String,
    /// Always `true`: a future restore must be explicitly confirmed again.
    pub rollback_requires_explicit_future_confirm: bool,
    /// Always `false` in this milestone.
    pub rollback_execution_supported: bool,
    /// Always `false` in this milestone.
    pub rollback_executed: bool,
    /// Always `false` in this milestone.
    pub safe_to_execute_now: bool,
    pub reason: String,
}

/// A post-write verification safety gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerPostWriteSafetyGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

/// A reason verification/rollback availability is blocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerPostWriteBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A non-fatal advisory observed while verifying.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerPostWriteWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerPostWriteSummary {
    pub total_gates: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub warning_count: usize,
    pub blocked_count: usize,
    pub verification_result: CodeWalkerPostWriteResult,
    pub rollback_available: bool,
    pub rollback_recommended: bool,
    pub rollback_executed: bool,
    pub modifies_archive: bool,
}

/// Read-only CodeWalker post-write verification + rollback plan. After a replace
/// attempt, it reads the local target file, the replace-apply report (T0.6.5),
/// the backup report (T0.5.1), the execution gate report (T0.6.4), and the dry
/// replace plan (T0.6.3); compares pre/post/backup hashes; classifies the result;
/// and builds a rollback PLAN pointing at the verified backup. It never restores
/// the backup, never modifies the target, never calls CodeWalker, never sends an
/// HTTP request, never uses POST, never executes an external tool, and never
/// parses RPF internals. `rollbackExecuted` and `rollbackExecutionAllowed` stay
/// `false`; the global `writerAllowed` stays `false` and the active adapter stays
/// `NullRpfAdapter`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerPostWriteVerifyReport {
    pub status: CodeWalkerPostWriteVerifyStatus,

    // ── Target ──────────────────────────────────────────────────────────────
    pub target_rpf: String,
    pub target_rpf_exists: bool,
    pub target_rpf_extension_valid: bool,
    pub target_current_sha256: Option<String>,
    pub target_current_size_bytes: Option<u64>,

    // ── Input report paths ──────────────────────────────────────────────────
    pub replace_apply_report_path: String,
    pub backup_report_path: String,
    pub execution_gate_report_path: String,
    pub dry_replace_plan_path: String,

    // ── Replace apply facts ─────────────────────────────────────────────────
    pub replace_apply_status: Option<String>,
    pub replace_requests_sent: bool,
    pub successful_replace_count: u64,
    pub failed_replace_count: u64,
    pub replace_apply_original_target_sha256: Option<String>,
    pub replace_apply_post_execution_target_sha256: Option<String>,

    // ── Hash comparisons (true/false/unknown via Option<bool>) ──────────────
    pub target_hash_matches_apply_report_post_hash: Option<bool>,
    pub target_hash_changed_from_pre_apply: Option<bool>,
    pub target_hash_matches_backup_original_hash: Option<bool>,

    // ── Backup facts ────────────────────────────────────────────────────────
    pub backup_file_path: Option<String>,
    pub backup_file_exists: bool,
    pub backup_hash_verified: bool,
    pub backup_safe_for_future_write: bool,
    pub backup_target_matches_target: Option<bool>,

    // ── Execution gate facts ────────────────────────────────────────────────
    pub execution_gate_was_eligible: bool,
    pub copied_test_archive_confirmed: bool,

    // ── Dry plan facts ──────────────────────────────────────────────────────
    pub dry_plan_planned_request_count: u64,

    // ── Verdict + rollback ──────────────────────────────────────────────────
    pub verification_result: CodeWalkerPostWriteResult,
    pub rollback_plan: CodeWalkerRollbackPlan,
    pub rollback_available: bool,
    pub rollback_recommended: bool,
    pub rollback_executed: bool,
    pub rollback_execution_allowed: bool,

    // ── Safety mirror (all conservative) ────────────────────────────────────
    pub http_requests_sent: bool,
    pub post_requests_sent: bool,
    pub replace_endpoint_called: bool,
    pub import_endpoint_called: bool,
    pub reload_services_called: bool,
    pub set_config_called: bool,
    pub external_tool_executed: bool,
    pub native_parser_used: bool,
    pub native_writer_used: bool,
    pub modifies_archive: bool,
    pub writer_allowed: bool,
    pub active_adapter_name: String,
    pub null_adapter_active: bool,

    pub gates: Vec<CodeWalkerPostWriteSafetyGate>,
    pub warnings: Vec<CodeWalkerPostWriteWarning>,
    pub blocked_items: Vec<CodeWalkerPostWriteBlockedItem>,
    pub summary: CodeWalkerPostWriteSummary,
}

// ── T0.6.7 controlled rollback restore from backup ───────────────────────────

/// Overall outcome of a rollback-restore pass.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeWalkerRollbackRestoreStatus {
    /// Every gate passed and the verified backup was copied over the target.
    Restored,
    /// A strict gate failed; the target was NOT modified.
    Blocked,
    /// A required input report/target was unusable; the target was NOT modified.
    InvalidInput,
    /// Gates passed but the copy/verify step failed; target state is reported.
    RestoreFailed,
}

/// A rollback-restore safety gate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerRollbackRestoreSafetyGate {
    pub name: String,
    pub passed: bool,
    pub severity: CodeWalkerApiSeverity,
    pub message: String,
}

/// A reason the restore was blocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerRollbackRestoreBlockedItem {
    pub component: String,
    pub reason: String,
    pub block_type: String,
}

/// A non-fatal advisory observed while restoring.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerRollbackRestoreWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerRollbackRestoreSummary {
    pub total_gates: usize,
    pub passed_gate_count: usize,
    pub blocking_gate_count: usize,
    pub warning_count: usize,
    pub blocked_count: usize,
    pub rollback_executed: bool,
    pub restored_target_matches_backup: Option<bool>,
    pub modifies_archive: bool,
}

/// Controlled rollback restore: copies a verified backup file back over a COPIED
/// TEST target archive — the first command that may modify a target archive on
/// disk. It runs only when the T0.6.6 post-write verification report has a ready
/// rollback plan, the T0.5.1 backup report is hash-verified and safe, the
/// recomputed backup hash matches the report, the target is a copied test archive
/// (never an original game path), `--execute-rollback` is given, and the exact
/// confirmation phrase matches. It never calls CodeWalker, never sends an HTTP
/// request, never uses POST, never executes an external tool, never parses RPF
/// internals, and never creates a backup. Global `writerAllowed` stays `false`
/// and the active adapter stays `NullRpfAdapter`; the only mutation is the gated
/// `copy_backup_over_target`. On any blocking gate failure the target is NOT
/// modified.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeWalkerRollbackRestoreReport {
    pub status: CodeWalkerRollbackRestoreStatus,

    // ── Target / inputs ─────────────────────────────────────────────────────
    pub target_rpf: String,
    pub backup_file_path: Option<String>,
    pub post_write_verify_report_path: String,
    pub backup_report_path: String,

    // ── Authorization ───────────────────────────────────────────────────────
    pub execute_rollback_requested: bool,
    pub confirmation_phrase_provided: bool,
    pub confirmation_phrase_matched: bool,
    pub expected_confirmation_phrase: String,

    // ── Target facts ────────────────────────────────────────────────────────
    pub target_rpf_exists: bool,
    pub target_rpf_extension_valid: bool,
    pub target_classification: String,
    pub copied_test_archive_confirmed: bool,
    pub target_not_original_game_archive: bool,

    // ── Backup facts ────────────────────────────────────────────────────────
    pub backup_file_exists: bool,
    pub backup_hash_verified: bool,
    pub backup_hash_matches_report: bool,
    pub backup_safe_for_future_write: bool,
    pub backup_target_matches_target: Option<bool>,
    pub backup_sha256: Option<String>,

    // ── Rollback plan facts (from T0.6.6) ───────────────────────────────────
    pub rollback_plan_ready: bool,
    pub rollback_available: bool,

    // ── Execution ───────────────────────────────────────────────────────────
    pub rollback_execution_allowed: bool,
    pub rollback_executed: bool,
    pub target_sha256_before: Option<String>,
    pub target_sha256_after: Option<String>,
    pub restored_target_matches_backup: Option<bool>,
    /// Always `copy_backup_over_target`.
    pub restore_method: String,

    // ── Safety mirror ───────────────────────────────────────────────────────
    pub http_requests_sent: bool,
    pub post_requests_sent: bool,
    pub replace_endpoint_called: bool,
    pub import_endpoint_called: bool,
    pub reload_services_called: bool,
    pub set_config_called: bool,
    pub external_tool_executed: bool,
    pub native_parser_used: bool,
    pub native_writer_used: bool,
    pub modifies_archive: bool,
    pub writer_allowed: bool,
    pub active_adapter_name: String,
    pub null_adapter_active: bool,

    pub gates: Vec<CodeWalkerRollbackRestoreSafetyGate>,
    pub warnings: Vec<CodeWalkerRollbackRestoreWarning>,
    pub blocked_items: Vec<CodeWalkerRollbackRestoreBlockedItem>,
    pub summary: CodeWalkerRollbackRestoreSummary,
}
