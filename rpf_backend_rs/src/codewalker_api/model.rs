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
