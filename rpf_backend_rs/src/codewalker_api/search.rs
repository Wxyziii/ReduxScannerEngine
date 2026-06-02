use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::detect::http_get;
use super::model::*;
use super::readiness::probe_codewalker_api_readiness;

use crate::rpf_adapter::contract::RpfAdapter;
use crate::rpf_adapter::null_adapter::NullRpfAdapter;

/// The single read-only search endpoint used by this milestone.
pub const SEARCH_ENDPOINT: &str = "/api/search-file";

// ── Tolerant views over existing JSON reports ───────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EntryManifestReportView {
    manifest: ManifestView,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ManifestView {
    entries: Vec<EntryView>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EntryView {
    archive_relative_path: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReadinessReportView {
    codewalker_api_ready_for_search: bool,
}

/// Percent-encode a query-parameter value (unreserved chars pass through).
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Normalize a result path: backslashes to forward slashes, trim. Case kept.
fn normalize_path(raw: &str) -> String {
    raw.trim().replace('\\', "/")
}

/// Final path component (basename) of a normalized path.
fn basename(normalized: &str) -> String {
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized)
        .to_string()
}

/// Tolerantly collect string paths from a search response body.
///
/// Supports: a JSON array of strings; a JSON array of objects with a path-like
/// field; or an object wrapping such an array under `results`/`matches`/`files`.
fn parse_search_results(body: &str) -> Vec<String> {
    let value: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    collect_paths(&value)
}

fn collect_paths(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        Value::Array(arr) => {
            for item in arr {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    Value::Object(_) => out.extend(path_from_object(item)),
                    _ => {}
                }
            }
        }
        Value::Object(obj) => {
            for key in ["results", "matches", "files", "paths", "data"] {
                if let Some(inner) = obj.get(key) {
                    out.extend(collect_paths(inner));
                }
            }
            // Also accept a single object that itself describes a path.
            if out.is_empty() {
                out.extend(path_from_object(value));
            }
        }
        _ => {}
    }
    out
}

fn path_from_object(value: &Value) -> Vec<String> {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Vec::new(),
    };
    for key in [
        "path",
        "fullPath",
        "archivePath",
        "file",
        "filePath",
        "name",
    ] {
        if let Some(Value::String(s)) = obj.get(key) {
            return vec![s.clone()];
        }
    }
    Vec::new()
}

fn gate(
    name: &str,
    passed: bool,
    severity: CodeWalkerApiSeverity,
    message: &str,
) -> CodeWalkerSearchSafetyGate {
    CodeWalkerSearchSafetyGate {
        name: name.to_string(),
        passed,
        severity,
        message: message.to_string(),
    }
}

/// Build a candidate with match classification against `arp` (archive-relative
/// path) and the requested `filename`.
fn classify(raw: &str, arp: &str, filename: &str) -> CodeWalkerSearchCandidate {
    let normalized = normalize_path(raw);
    let cand_file = basename(&normalized);
    let exact = normalized == arp;
    let suffix = !exact && normalized.ends_with(arp);
    let matches_filename = cand_file == filename;
    let confidence = if exact {
        CodeWalkerSearchConfidence::Exact
    } else if suffix {
        CodeWalkerSearchConfidence::Suffix
    } else if matches_filename {
        CodeWalkerSearchConfidence::FilenameOnly
    } else {
        CodeWalkerSearchConfidence::None
    };
    CodeWalkerSearchCandidate {
        raw_path: raw.to_string(),
        normalized_path: normalized,
        filename: cand_file,
        matches_filename,
        matches_archive_relative_path_suffix: suffix,
        confidence,
        selected: false,
    }
}

/// Read-only CodeWalker search + target-resolution planner.
///
/// Reads the T0.5.7 entry manifest report, and for each entry issues a safe
/// GET `/api/search-file?filename=<basename>`, mapping results back to the
/// archive-relative path. Never calls replace/import/reload-services/set-config,
/// never issues a POST or any mutation, never executes CodeWalker, never opens or
/// modifies an RPF archive. `canWriteArchive` and `writerAllowed` stay `false`.
/// Offline yields a valid all-unresolved report rather than an error.
pub fn build_codewalker_search_resolve_report(
    entry_manifest_report_path: &Path,
    base_url: Option<&str>,
    readiness_report_path: Option<&Path>,
) -> Result<CodeWalkerSearchResolveReport, String> {
    // Active adapter facts come from the real, safe adapter — never CodeWalker.
    let adapter = NullRpfAdapter::new();
    let active_adapter_name = adapter.name().to_string();
    let null_adapter_active = !adapter.capabilities().can_write_archive;

    let mut warnings: Vec<CodeWalkerSearchWarning> = Vec::new();
    let mut blocked_items: Vec<CodeWalkerSearchBlockedItem> = Vec::new();

    // ── Load the entry manifest report (tolerant) ──────────────────────────
    let manifest_present = entry_manifest_report_path.is_file();
    let (entries, manifest_loaded) = if manifest_present {
        match fs::read_to_string(entry_manifest_report_path) {
            Ok(text) => match serde_json::from_str::<EntryManifestReportView>(&text) {
                Ok(view) => {
                    let paths: Vec<String> = view
                        .manifest
                        .entries
                        .into_iter()
                        .map(|e| e.archive_relative_path)
                        .filter(|p| !p.trim().is_empty())
                        .collect();
                    (paths, true)
                }
                Err(e) => {
                    warnings.push(CodeWalkerSearchWarning {
                        code: "entry_manifest_parse_failed".to_string(),
                        message: format!("Could not parse entry manifest report: {e}"),
                    });
                    (Vec::new(), false)
                }
            },
            Err(e) => {
                warnings.push(CodeWalkerSearchWarning {
                    code: "entry_manifest_read_failed".to_string(),
                    message: format!("Could not read entry manifest report: {e}"),
                });
                (Vec::new(), false)
            }
        }
    } else {
        blocked_items.push(CodeWalkerSearchBlockedItem {
            component: "input".to_string(),
            reason: "Entry manifest report file was not found.".to_string(),
            block_type: "entry_manifest_report_missing".to_string(),
        });
        (Vec::new(), false)
    };

    // ── Optional readiness report context (read-only) ──────────────────────
    let (readiness_report_path_str, readiness_checked) = match readiness_report_path {
        Some(p) => {
            let s = p.display().to_string();
            if p.is_file() {
                match fs::read_to_string(p)
                    .ok()
                    .and_then(|t| serde_json::from_str::<ReadinessReportView>(&t).ok())
                {
                    Some(_) => (Some(s), true),
                    None => {
                        warnings.push(CodeWalkerSearchWarning {
                            code: "readiness_report_unparsable".to_string(),
                            message: "Readiness report could not be parsed; ignoring.".to_string(),
                        });
                        (Some(s), false)
                    }
                }
            } else {
                warnings.push(CodeWalkerSearchWarning {
                    code: "readiness_report_missing".to_string(),
                    message: "Readiness report path was provided but not found.".to_string(),
                });
                (Some(s), false)
            }
        }
        None => (None, false),
    };

    // ── Live readiness probe (GET-only) ────────────────────────────────────
    let readiness = probe_codewalker_api_readiness(base_url)?;
    let normalized_base_url = readiness.normalized_base_url.clone();
    let raw_base = readiness.base_url.clone();
    let reachable = readiness.codewalker_api_reachable;
    let ready_for_search = readiness.codewalker_api_ready_for_search;

    if !reachable {
        blocked_items.push(CodeWalkerSearchBlockedItem {
            component: "codewalker".to_string(),
            reason: "CodeWalker.API was not reachable; no searches were run.".to_string(),
            block_type: "codewalker_api_offline".to_string(),
        });
    } else if !ready_for_search {
        warnings.push(CodeWalkerSearchWarning {
            code: "codewalker_not_ready".to_string(),
            message: "CodeWalker.API reachable but not confirmed ready for search.".to_string(),
        });
    }

    // ── Per-entry search + resolution ──────────────────────────────────────
    let mut targets: Vec<CodeWalkerSearchTarget> = Vec::new();
    let mut resolved_targets: Vec<CodeWalkerResolvedTarget> = Vec::new();
    let mut unresolved_targets: Vec<CodeWalkerUnresolvedTarget> = Vec::new();
    let mut ambiguous_targets: Vec<CodeWalkerUnresolvedTarget> = Vec::new();
    let mut search_requests: Vec<CodeWalkerSearchRequest> = Vec::new();

    for arp in &entries {
        let arp = normalize_path(arp);
        let filename = basename(&arp);
        let search_url_path = format!("{SEARCH_ENDPOINT}?filename={}", url_encode(&filename));

        let mut candidates: Vec<CodeWalkerSearchCandidate> = Vec::new();

        if reachable {
            let full_url = format!("{normalized_base_url}{search_url_path}");
            match http_get(&full_url) {
                Ok(resp) => {
                    search_requests.push(CodeWalkerSearchRequest {
                        method: "GET".to_string(),
                        url: full_url,
                        requested_filename: filename.clone(),
                        http_status: Some(resp.status),
                        succeeded: true,
                        detail: Some(format!("HTTP {}", resp.status)),
                    });
                    for raw in parse_search_results(&resp.body) {
                        candidates.push(classify(&raw, &arp, &filename));
                    }
                }
                Err(e) => {
                    search_requests.push(CodeWalkerSearchRequest {
                        method: "GET".to_string(),
                        url: full_url,
                        requested_filename: filename.clone(),
                        http_status: None,
                        succeeded: false,
                        detail: Some(e.clone()),
                    });
                    warnings.push(CodeWalkerSearchWarning {
                        code: "search_request_failed".to_string(),
                        message: format!("Search for {filename} failed: {e}"),
                    });
                }
            }
        }

        // Resolution rules.
        let exact_idx: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.confidence == CodeWalkerSearchConfidence::Exact)
            .map(|(i, _)| i)
            .collect();
        let suffix_idx: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.confidence == CodeWalkerSearchConfidence::Suffix)
            .map(|(i, _)| i)
            .collect();
        let filename_only = candidates
            .iter()
            .any(|c| c.confidence == CodeWalkerSearchConfidence::FilenameOnly);

        let exact_match_found = !exact_idx.is_empty();
        let suffix_match_found = !suffix_idx.is_empty();

        let mut resolved = false;
        let mut ambiguous = false;
        let mut match_type = CodeWalkerSearchConfidence::None;
        let mut selected_candidate: Option<String> = None;
        let reason: String;

        if !reachable {
            reason = "CodeWalker.API offline; target unresolved.".to_string();
        } else if exact_idx.len() == 1 {
            let i = exact_idx[0];
            candidates[i].selected = true;
            selected_candidate = Some(candidates[i].normalized_path.clone());
            match_type = CodeWalkerSearchConfidence::Exact;
            resolved = true;
            reason = "Exactly one exact match.".to_string();
        } else if exact_idx.is_empty() && suffix_idx.len() == 1 {
            let i = suffix_idx[0];
            candidates[i].selected = true;
            selected_candidate = Some(candidates[i].normalized_path.clone());
            match_type = CodeWalkerSearchConfidence::Suffix;
            resolved = true;
            reason = "Exactly one suffix match.".to_string();
        } else if exact_idx.len() + suffix_idx.len() > 1 {
            ambiguous = true;
            reason = "Multiple matching candidates; ambiguous.".to_string();
        } else if filename_only {
            reason = "Only filename-only candidates; not enough to resolve.".to_string();
        } else {
            reason = "No matching candidate found.".to_string();
        }

        if resolved {
            resolved_targets.push(CodeWalkerResolvedTarget {
                archive_relative_path: arp.clone(),
                selected_candidate: selected_candidate.clone().unwrap_or_default(),
                match_type,
            });
        } else {
            let u = CodeWalkerUnresolvedTarget {
                archive_relative_path: arp.clone(),
                requested_filename: filename.clone(),
                reason: reason.clone(),
                candidate_count: candidates.len(),
            };
            if ambiguous {
                ambiguous_targets.push(u.clone());
            }
            unresolved_targets.push(u);
        }

        targets.push(CodeWalkerSearchTarget {
            archive_relative_path: arp,
            requested_filename: filename,
            search_url_path,
            candidates,
            exact_match_found,
            suffix_match_found,
            ambiguous,
            resolved,
            match_type,
            selected_candidate,
            reason,
        });
    }

    // ── Status ─────────────────────────────────────────────────────────────
    let status = if !manifest_loaded {
        CodeWalkerSearchResolveStatus::InvalidInput
    } else if !reachable {
        CodeWalkerSearchResolveStatus::Offline
    } else if !ready_for_search {
        CodeWalkerSearchResolveStatus::NotReady
    } else {
        CodeWalkerSearchResolveStatus::Completed
    };

    // ── Safety gates ─────────────────────────────────────────────────────────
    let gates = vec![
        gate(
            "entry_manifest_report_present",
            manifest_present,
            if manifest_present {
                CodeWalkerApiSeverity::Info
            } else {
                CodeWalkerApiSeverity::Blocking
            },
            "The entry manifest report file was present.",
        ),
        gate(
            "entry_manifest_loaded",
            manifest_loaded,
            if manifest_loaded {
                CodeWalkerApiSeverity::Info
            } else {
                CodeWalkerApiSeverity::Blocking
            },
            "The entry manifest report parsed successfully.",
        ),
        gate(
            "codewalker_readiness_context_loaded_or_not_required",
            readiness_report_path.is_none() || readiness_checked,
            CodeWalkerApiSeverity::Info,
            "Readiness context was loaded, or none was required.",
        ),
        gate(
            "codewalker_api_reachable",
            reachable,
            if reachable {
                CodeWalkerApiSeverity::Info
            } else {
                CodeWalkerApiSeverity::Warning
            },
            "CodeWalker.API responded to a read-only probe.",
        ),
        gate(
            "codewalker_ready_for_search",
            ready_for_search,
            if ready_for_search {
                CodeWalkerApiSeverity::Info
            } else {
                CodeWalkerApiSeverity::Warning
            },
            "CodeWalker.API reported readiness for search.",
        ),
        gate(
            "search_endpoint_called_get_only",
            true,
            CodeWalkerApiSeverity::Info,
            "Only GET was used for the search endpoint.",
        ),
        gate(
            "no_post_requests_used",
            true,
            CodeWalkerApiSeverity::Info,
            "No POST request was issued.",
        ),
        gate(
            "write_endpoints_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "No write endpoint was called.",
        ),
        gate(
            "replace_endpoint_not_called",
            true,
            CodeWalkerApiSeverity::Info,
            "/api/replace-file was not called.",
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
            "archive_not_modified",
            true,
            CodeWalkerApiSeverity::Info,
            "No RPF archive was opened or modified.",
        ),
    ];

    blocked_items.push(CodeWalkerSearchBlockedItem {
        component: "writer".to_string(),
        reason: "The real RPF writer is not implemented.".to_string(),
        block_type: "real_rpf_writer_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerSearchBlockedItem {
        component: "parser".to_string(),
        reason: "Native RPF parsing is not implemented.".to_string(),
        block_type: "native_rpf_parser_not_implemented".to_string(),
    });
    blocked_items.push(CodeWalkerSearchBlockedItem {
        component: "adapter".to_string(),
        reason: "The active adapter is NullRpfAdapter and cannot write.".to_string(),
        block_type: "active_adapter_cannot_write".to_string(),
    });

    let passed_gate_count = gates.iter().filter(|g| g.passed).count();
    let blocking_gate_count = gates
        .iter()
        .filter(|g| !g.passed && g.severity == CodeWalkerApiSeverity::Blocking)
        .count();

    let summary = CodeWalkerSearchSummary {
        target_count: targets.len(),
        resolved_count: resolved_targets.len(),
        unresolved_count: unresolved_targets.len(),
        ambiguous_count: ambiguous_targets.len(),
        search_request_count: search_requests.len(),
        passed_gate_count,
        blocking_gate_count,
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        reachable,
        writer_allowed: false,
    };

    Ok(CodeWalkerSearchResolveReport {
        status,
        base_url: raw_base,
        normalized_base_url,
        entry_manifest_report_path: entry_manifest_report_path.display().to_string(),
        readiness_report_path: readiness_report_path_str,
        readiness_checked,
        codewalker_api_reachable: reachable,
        codewalker_api_ready_for_search: ready_for_search,
        search_endpoint_used: SEARCH_ENDPOINT.to_string(),
        targets,
        resolved_targets,
        unresolved_targets,
        ambiguous_targets,
        search_requests,
        get_requests_only: true,
        post_requests_used: false,
        replace_endpoint_called: false,
        import_endpoint_called: false,
        reload_services_called: false,
        set_config_called: false,
        mutation_endpoints_called: false,
        external_tool_executed: false,
        modifies_archive: false,
        writer_allowed: false,
        can_write_archive: false,
        active_adapter_name,
        blocked_items,
        warnings,
        safety_gates: gates,
        summary,
    })
}
