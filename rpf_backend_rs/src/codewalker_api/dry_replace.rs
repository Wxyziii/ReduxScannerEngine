use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::model::*;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// The future replace endpoint this milestone only ever MODELS, never calls.
pub const REPLACE_ENDPOINT: &str = "/api/replace-file";
/// The HTTP method the future replace would use. Modelled, never issued.
pub const REPLACE_METHOD: &str = "POST";
/// The selected future writer route. Locked to CodeWalker.API.
pub const SELECTED_WRITER_ROUTE: &str = "CodeWalker.API";
/// Names the CodeWalker.API `/api/replace-file` request contract this plan emits
/// (`ReplaceFileForm`: required `localFilePath` + `rpfFilePath`). Discovered in
/// T0.6.15 from the live HTTP 400 "Invalid or missing localFilePath/rpfFilePath".
pub const REPLACE_API_CONTRACT_NAME: &str = "codewalker_replace_file_v1";

/// Make a path absolute (without the Windows verbatim `\\?\` prefix that
/// `canonicalize` adds), joining the current working directory when relative.
fn to_absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|c| c.join(p))
            .unwrap_or_else(|_| p.to_path_buf())
    }
}

// ── Tolerant views over existing JSON reports ───────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EntryManifestReportView {
    manifest: ManifestView,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ManifestView {
    entries: Vec<ManifestEntryView>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ManifestEntryView {
    archive_relative_path: String,
    bundle_file_relative_path: String,
    bundle_file_absolute_path: String,
    size_bytes: u64,
    sha256: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ResolveReportView {
    resolved_targets: Vec<ResolvedTargetView>,
    unresolved_targets: Vec<UnresolvedTargetView>,
    ambiguous_targets: Vec<UnresolvedTargetView>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ResolvedTargetView {
    archive_relative_path: String,
    selected_candidate: String,
    match_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct UnresolvedTargetView {
    archive_relative_path: String,
}

/// Minimal tolerant view of the optional writer-permission report.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PermissionReportView {
    writer_allowed: bool,
}

/// Normalize a path: backslashes to forward slashes, trim. Case preserved.
fn normalize_path(raw: &str) -> String {
    raw.trim().replace('\\', "/")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerDryReplaceSafetyGate {
    CodeWalkerDryReplaceSafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// What a resolve report says about one archive-relative path.
enum Resolution {
    Resolved {
        path: String,
        match_type: Option<String>,
    },
    Ambiguous,
    Unresolved,
}

fn classify_resolution(view: &ResolveReportView, arp: &str) -> Resolution {
    if let Some(r) = view
        .resolved_targets
        .iter()
        .find(|t| normalize_path(&t.archive_relative_path) == arp)
    {
        return Resolution::Resolved {
            path: normalize_path(&r.selected_candidate),
            match_type: r.match_type.clone(),
        };
    }
    if view
        .ambiguous_targets
        .iter()
        .any(|t| normalize_path(&t.archive_relative_path) == arp)
    {
        return Resolution::Ambiguous;
    }
    Resolution::Unresolved
}

/// Resolve the providing bundle file to an absolute path. Prefers the manifest's
/// recorded absolute path; falls back to `<bundle_dir>/<relative>`.
fn resolve_bundle_file(bundle_dir: &Path, entry: &ManifestEntryView) -> PathBuf {
    let abs = PathBuf::from(&entry.bundle_file_absolute_path);
    if !entry.bundle_file_absolute_path.trim().is_empty() && abs.is_file() {
        return abs;
    }
    if !entry.bundle_file_relative_path.trim().is_empty() {
        return bundle_dir.join(&entry.bundle_file_relative_path);
    }
    abs
}

/// Build a read-only CodeWalker dry replace plan.
///
/// Reads the T0.5.7 entry manifest report, the T0.6.2 resolve report, and an
/// optional T0.5.8 writer-permission report, plus the providing bundle files.
/// Produces MODELLED `/api/replace-file` payloads describing what a future writer
/// would send. Issues NO HTTP request, never uses POST, never calls replace/
/// import/reload-services/set-config or any mutation endpoint, never executes
/// CodeWalker or any external tool, and never opens or modifies an RPF archive.
/// `readyForExecution`, `writerAllowed`, and `codewalkerExecutionAllowed` all
/// stay `false`. A blocked item never fails the whole report.
pub fn build_codewalker_dry_replace_plan(
    bundle_dir: &Path,
    entry_manifest_report_path: &Path,
    resolve_report_path: &Path,
    permission_report_path: Option<&Path>,
) -> Result<CodeWalkerDryReplacePlanReport, String> {
    // Active adapter facts come from the real, safe adapter — never CodeWalker.
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerDryReplaceWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerDryReplaceBlockedItem> = Vec::new();

    // ── Bundle dir presence ─────────────────────────────────────────────────
    let bundle_dir_present = bundle_dir.is_dir();
    let bundle_files_dir = bundle_dir.join("files");
    let bundle_files_dir_present = bundle_files_dir.is_dir();
    if !bundle_dir_present {
        blocked_items.push(CodeWalkerDryReplaceBlockedItem {
            component: "input".to_string(),
            reason: "Bundle directory was not found.".to_string(),
            block_type: "bundle_dir_missing".to_string(),
        });
    } else if !bundle_files_dir_present {
        blocked_items.push(CodeWalkerDryReplaceBlockedItem {
            component: "input".to_string(),
            reason: "Bundle files directory (<bundle>/files) was not found.".to_string(),
            block_type: "bundle_files_dir_missing".to_string(),
        });
    }

    // ── Load entry manifest report (tolerant) ───────────────────────────────
    let manifest_present = entry_manifest_report_path.is_file();
    let (entries, manifest_loaded): (Vec<ManifestEntryView>, bool) = if manifest_present {
        match fs::read_to_string(entry_manifest_report_path)
            .ok()
            .and_then(|t| serde_json::from_str::<EntryManifestReportView>(&t).ok())
        {
            Some(view) => (view.manifest.entries, true),
            None => {
                warnings.push(CodeWalkerDryReplaceWarning {
                    code: "entry_manifest_parse_failed".to_string(),
                    message: "Could not read/parse the entry manifest report.".to_string(),
                });
                (Vec::new(), false)
            }
        }
    } else {
        blocked_items.push(CodeWalkerDryReplaceBlockedItem {
            component: "input".to_string(),
            reason: "Entry manifest report file was not found.".to_string(),
            block_type: "entry_manifest_report_missing".to_string(),
        });
        (Vec::new(), false)
    };

    // ── Load resolve report (tolerant) ──────────────────────────────────────
    let resolve_present = resolve_report_path.is_file();
    let (resolve_view, resolve_loaded): (ResolveReportView, bool) = if resolve_present {
        match fs::read_to_string(resolve_report_path)
            .ok()
            .and_then(|t| serde_json::from_str::<ResolveReportView>(&t).ok())
        {
            Some(view) => (view, true),
            None => {
                warnings.push(CodeWalkerDryReplaceWarning {
                    code: "resolve_report_parse_failed".to_string(),
                    message: "Could not read/parse the resolve report.".to_string(),
                });
                (ResolveReportView::default(), false)
            }
        }
    } else {
        blocked_items.push(CodeWalkerDryReplaceBlockedItem {
            component: "input".to_string(),
            reason: "Resolve report file was not found.".to_string(),
            block_type: "resolve_report_missing".to_string(),
        });
        (ResolveReportView::default(), false)
    };

    // ── Optional permission report (read-only context) ──────────────────────
    let (permission_report_path_str, permission_loaded) = match permission_report_path {
        Some(p) => {
            let s = p.display().to_string();
            if p.is_file() {
                match fs::read_to_string(p)
                    .ok()
                    .and_then(|t| serde_json::from_str::<PermissionReportView>(&t).ok())
                {
                    Some(_) => (Some(s), true),
                    None => {
                        warnings.push(CodeWalkerDryReplaceWarning {
                            code: "permission_report_unparsable".to_string(),
                            message: "Permission report could not be parsed; ignoring.".to_string(),
                        });
                        (Some(s), false)
                    }
                }
            } else {
                warnings.push(CodeWalkerDryReplaceWarning {
                    code: "permission_report_missing".to_string(),
                    message: "Permission report path was provided but not found.".to_string(),
                });
                (Some(s), false)
            }
        }
        None => (None, false),
    };

    // ── Per-entry planning ──────────────────────────────────────────────────
    let mut items: Vec<CodeWalkerDryReplaceItem> = Vec::new();
    let mut planned_requests: Vec<CodeWalkerDryReplacePayload> = Vec::new();
    let mut any_ambiguous_blocked = false;

    for entry in &entries {
        let arp = normalize_path(&entry.archive_relative_path);
        let resolution = classify_resolution(&resolve_view, &arp);

        let (resolved, ambiguous, resolved_path, match_type) = match &resolution {
            Resolution::Resolved { path, match_type } => {
                (true, false, Some(path.clone()), match_type.clone())
            }
            Resolution::Ambiguous => (false, true, None, None),
            Resolution::Unresolved => (false, false, None, None),
        };

        // Bundle file facts.
        let bundle_abs = resolve_bundle_file(bundle_dir, entry);
        let bundle_file_exists = bundle_abs.is_file();
        let (size_bytes, file_sha256) = if bundle_file_exists {
            match fs::read(&bundle_abs) {
                Ok(bytes) => (bytes.len() as u64, Some(sha256_hex(&bytes))),
                Err(_) => (0, None),
            }
        } else {
            (entry.size_bytes, None)
        };
        let manifest_sha256 = entry.sha256.clone();
        let hash_matches_manifest = match (&file_sha256, &manifest_sha256) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            _ => false,
        };

        // Decide validity / block reason (item-level only — never fails report).
        let mut blocked_reason: Option<String> = None;
        if ambiguous {
            blocked_reason = Some("CodeWalker target is ambiguous.".to_string());
            any_ambiguous_blocked = true;
        } else if !resolved {
            blocked_reason = Some("CodeWalker target is unresolved.".to_string());
        } else if !bundle_file_exists {
            blocked_reason = Some("Bundle file does not exist.".to_string());
        } else if manifest_sha256.is_none() {
            blocked_reason = Some("Manifest has no SHA-256 to verify against.".to_string());
        } else if !hash_matches_manifest {
            blocked_reason = Some("Bundle file SHA-256 does not match the manifest.".to_string());
        }

        let valid_for_future_replace = blocked_reason.is_none();

        if let Some(reason) = &blocked_reason {
            blocked_items.push(CodeWalkerDryReplaceBlockedItem {
                component: "item".to_string(),
                reason: format!("{arp}: {reason}"),
                block_type: "item_not_ready_for_replace".to_string(),
            });
        }

        let planned_payload = if valid_for_future_replace {
            // CodeWalker.API contract: localFilePath must be an absolute local
            // path; rpfFilePath is the resolved in-archive entry path.
            let local_abs = to_absolute(&bundle_abs);
            let local_file_path = local_abs.display().to_string();
            let local_file_path_is_absolute = local_abs.is_absolute();
            let local_file_path_exists = local_abs.is_file();
            let codewalker_target_path = resolved_path.clone().unwrap_or_default();
            let request_schema_validated = local_file_path_is_absolute
                && local_file_path_exists
                && !codewalker_target_path.trim().is_empty();
            let payload = CodeWalkerDryReplacePayload {
                endpoint: REPLACE_ENDPOINT.to_string(),
                method: REPLACE_METHOD.to_string(),
                api_contract_name: REPLACE_API_CONTRACT_NAME.to_string(),
                actual_request_payload: CodeWalkerReplaceActualPayload {
                    local_file_path: local_file_path.clone(),
                    rpf_file_path: codewalker_target_path.clone(),
                },
                local_file_path,
                local_file_path_is_absolute,
                local_file_path_exists,
                codewalker_target_path,
                request_schema_validated,
                rpf_path: resolved_path.clone(),
                archive_path: resolved_path.clone(),
                source_file_path: bundle_abs.display().to_string(),
                archive_relative_path: arp.clone(),
                dry_run_only: true,
            };
            planned_requests.push(payload.clone());
            Some(payload)
        } else {
            None
        };

        let source_file = CodeWalkerDryReplaceSourceFile {
            bundle_file_relative_path: entry.bundle_file_relative_path.clone(),
            bundle_file_absolute_path: bundle_abs.display().to_string(),
            bundle_file_exists,
            bundle_file_size_bytes: size_bytes,
            bundle_file_sha256: file_sha256.clone(),
            manifest_sha256: manifest_sha256.clone(),
            hash_matches_manifest,
        };

        let resolved_target = CodeWalkerDryReplaceResolvedTarget {
            archive_relative_path: arp.clone(),
            codewalker_resolved_path: resolved_path.clone(),
            match_type: match_type.clone(),
            resolved,
            ambiguous,
        };

        items.push(CodeWalkerDryReplaceItem {
            archive_relative_path: arp,
            codewalker_resolved_path: resolved_path,
            bundle_file_relative_path: entry.bundle_file_relative_path.clone(),
            bundle_file_absolute_path: bundle_abs.display().to_string(),
            bundle_file_exists,
            bundle_file_size_bytes: size_bytes,
            bundle_file_sha256: file_sha256,
            manifest_sha256,
            hash_matches_manifest,
            exact_or_suffix_match_type: match_type,
            source_file,
            resolved_target,
            planned_payload,
            valid_for_future_replace,
            blocked_reason,
        });
    }

    // ── Tallies ──────────────────────────────────────────────────────────────
    let item_count = items.len();
    let valid_item_count = items.iter().filter(|i| i.valid_for_future_replace).count();
    let blocked_item_count = item_count - valid_item_count;
    let resolved_target_count = items.iter().filter(|i| i.resolved_target.resolved).count();
    let hash_match_count = items.iter().filter(|i| i.hash_matches_manifest).count();
    let all_bundle_files_present = item_count > 0 && items.iter().all(|i| i.bundle_file_exists);
    let all_hashes_match = item_count > 0 && items.iter().all(|i| i.hash_matches_manifest);
    let all_resolved = item_count > 0 && items.iter().all(|i| i.resolved_target.resolved);

    // ── Status ─────────────────────────────────────────────────────────────
    let inputs_ok = bundle_dir_present && manifest_loaded && resolve_loaded;
    let status = if !inputs_ok {
        CodeWalkerDryReplacePlanStatus::InvalidInput
    } else if item_count > 0 && valid_item_count == item_count {
        CodeWalkerDryReplacePlanStatus::Planned
    } else if valid_item_count > 0 {
        CodeWalkerDryReplacePlanStatus::Partial
    } else {
        CodeWalkerDryReplacePlanStatus::Blocked
    };

    // ── Safety gates ─────────────────────────────────────────────────────────
    let sev = |ok: bool, blocking: bool| {
        if ok {
            CodeWalkerApiSeverity::Info
        } else if blocking {
            CodeWalkerApiSeverity::Blocking
        } else {
            CodeWalkerApiSeverity::Warning
        }
    };
    let gates = vec![
        gate(
            "bundle_dir_present",
            bundle_dir_present,
            sev(bundle_dir_present, true),
            "The bundle directory was present.",
        ),
        gate(
            "bundle_files_dir_present",
            bundle_files_dir_present,
            sev(bundle_files_dir_present, true),
            "The bundle files directory (<bundle>/files) was present.",
        ),
        gate(
            "entry_manifest_loaded",
            manifest_loaded,
            sev(manifest_loaded, true),
            "The entry manifest report parsed successfully.",
        ),
        gate(
            "resolve_report_loaded",
            resolve_loaded,
            sev(resolve_loaded, true),
            "The resolve report parsed successfully.",
        ),
        gate(
            "permission_report_loaded_or_not_required",
            permission_report_path.is_none() || permission_loaded,
            CodeWalkerApiSeverity::Info,
            "Permission context was loaded, or none was required.",
        ),
        gate(
            "all_manifest_entries_checked",
            true,
            CodeWalkerApiSeverity::Info,
            "Every manifest entry was evaluated for a planned replace.",
        ),
        gate(
            "resolved_targets_required",
            all_resolved,
            sev(all_resolved, false),
            "Every manifest entry resolved to a CodeWalker target.",
        ),
        gate(
            "ambiguous_targets_blocked",
            !any_ambiguous_blocked,
            sev(!any_ambiguous_blocked, false),
            "No ambiguous target was planned for replace.",
        ),
        gate(
            "bundle_files_present",
            all_bundle_files_present,
            sev(all_bundle_files_present, false),
            "Every planned source bundle file exists.",
        ),
        gate(
            "bundle_hashes_match_manifest",
            all_hashes_match,
            sev(all_hashes_match, false),
            "Every bundle file SHA-256 matched the manifest.",
        ),
        gate(
            "dry_run_only",
            true,
            CodeWalkerApiSeverity::Info,
            "This is a dry-run plan only; nothing was applied.",
        ),
        gate(
            "no_http_requests_sent",
            true,
            CodeWalkerApiSeverity::Info,
            "No HTTP request of any kind was sent.",
        ),
        gate(
            "no_post_requests_sent",
            true,
            CodeWalkerApiSeverity::Info,
            "No POST request was issued.",
        ),
        gate(
            "replace_endpoint_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/replace-file was modelled only, never called.",
        ),
        gate(
            "import_endpoint_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/import was not called.",
        ),
        gate(
            "reload_services_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/reload-services was not called.",
        ),
        gate(
            "set_config_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/set-config was not called.",
        ),
        gate(
            "mutation_endpoints_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "No mutation endpoint was called.",
        ),
        gate(
            "null_adapter_still_active",
            null_adapter_active,
            CodeWalkerApiSeverity::Info,
            "The active adapter remains NullRpfAdapter.",
        ),
        gate(
            "writer_allowed_false",
            true,
            CodeWalkerApiSeverity::Info,
            "Writing remains disabled (writerAllowed is false).",
        ),
        gate(
            "codewalker_execution_allowed_false",
            true,
            CodeWalkerApiSeverity::Info,
            "CodeWalker execution remains disabled.",
        ),
        gate(
            "archive_not_modified",
            true,
            CodeWalkerApiSeverity::Info,
            "No RPF archive was opened or modified.",
        ),
    ];

    blocked_items.push(CodeWalkerDryReplaceBlockedItem {
        component: "writer".to_string(),
        reason: "The real RPF writer is not implemented.".to_string(),
        block_type: "real_rpf_writer_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerDryReplaceBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerDryReplaceBlockedItem {
        component: "codewalker".to_string(),
        reason: "CodeWalker execution is not implemented and not enabled.".to_string(),
        block_type: "codewalker_execution_not_enabled".to_string(),
    });
    blocked_items.push(CodeWalkerDryReplaceBlockedItem {
        component: "adapter".to_string(),
        reason: "The active adapter is NullRpfAdapter and cannot write.".to_string(),
        block_type: "active_adapter_cannot_write".to_string(),
    });

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerDryReplaceSummary {
        item_count,
        valid_item_count,
        blocked_item_count,
        planned_request_count: planned_requests.len(),
        resolved_target_count,
        hash_match_count,
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        // Always false in this milestone, regardless of how clean the plan is.
        ready_for_execution: false,
        writer_allowed: false,
    };

    Ok(CodeWalkerDryReplacePlanReport {
        status,
        bundle_dir: bundle_dir.display().to_string(),
        entry_manifest_report_path: entry_manifest_report_path.display().to_string(),
        resolve_report_path: resolve_report_path.display().to_string(),
        permission_report_path: permission_report_path_str,
        selected_writer_route: SELECTED_WRITER_ROUTE.to_string(),
        active_adapter_name,
        dry_run_only: true,
        ready_for_execution: false,
        writer_allowed: false,
        codewalker_execution_allowed: false,
        can_write_archive: false,
        planned_endpoint: REPLACE_ENDPOINT.to_string(),
        planned_http_method: REPLACE_METHOD.to_string(),
        items,
        planned_requests,
        blocked_items,
        warnings,
        safety_gates: gates,
        summary,
        post_requests_sent: false,
        get_requests_sent: false,
        replace_endpoint_called: false,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        mutation_endpoints_called: false,
        external_tool_executed: false,
        modifies_archive: false,
        real_writer_implemented: false,
        native_parser_implemented: false,
    })
}
