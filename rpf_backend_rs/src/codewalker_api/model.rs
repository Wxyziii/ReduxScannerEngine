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
