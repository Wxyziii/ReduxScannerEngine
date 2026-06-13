use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::model::*;

// ── Tolerant views over the input reports ───────────────────────────────────
//
// Every field is optional/defaulted: an older/newer/unexpected report shape must
// never crash the summarizer. We only read what we need.

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CompatProbeView {
    base_url: Option<String>,
    root_http_status: Option<u16>,
    compatible_for_search: Option<bool>,
    compatible_for_dry_replace_planning: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReadinessView {
    base_url: Option<String>,
    codewalker_api_reachable: Option<bool>,
    codewalker_api_ready_for_search: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ResolveSummaryView {
    target_count: u64,
    resolved_count: u64,
    unresolved_count: u64,
    ambiguous_count: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ResolveView {
    codewalker_api_reachable: Option<bool>,
    summary: ResolveSummaryView,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DryPlanView {
    planned_requests: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ExecutionGateView {
    target_rpf: Option<String>,
    codewalker_execution_eligible: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReplaceApplyView {
    target_rpf: Option<String>,
    base_url: Option<String>,
    replace_requests_sent: bool,
    successful_replace_count: u64,
    failed_replace_count: u64,
    /// "changed" | "unchanged" | "unknown"
    target_hash_changed: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PostWriteVerifyView {
    target_rpf: Option<String>,
    /// e.g. "execution_succeeded_target_changed", "..._suspicious", etc.
    verification_result: Option<String>,
    target_hash_changed_from_pre_apply: Option<bool>,
    rollback_available: bool,
    rollback_executed: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RollbackRestoreView {
    target_rpf: Option<String>,
    /// "restored" | "blocked" | "invalid_input" | "restore_failed"
    status: Option<String>,
    rollback_available: bool,
    rollback_executed: bool,
    restored_target_matches_backup: Option<bool>,
}

/// Result of trying to read+parse one optional report path.
struct Loaded<T> {
    provided: bool,
    /// File existed and parsed as the expected view.
    loaded: bool,
    /// File existed but could not be parsed as JSON at all.
    malformed: bool,
    value: T,
}

/// Tolerantly read a report path into a typed view. Never returns an error:
/// missing/unreadable/malformed files degrade into `loaded == false`.
fn load_view<T>(path: Option<&Path>) -> Loaded<T>
where
    T: for<'de> Deserialize<'de> + Default,
{
    match path {
        None => Loaded {
            provided: false,
            loaded: false,
            malformed: false,
            value: T::default(),
        },
        Some(p) => {
            let text = match fs::read_to_string(p) {
                Ok(t) => t,
                Err(_) => {
                    return Loaded {
                        provided: true,
                        loaded: false,
                        malformed: false,
                        value: T::default(),
                    };
                }
            };
            // First confirm it is JSON at all (distinguishes "malformed" from
            // "valid JSON but a different shape").
            let is_json = serde_json::from_str::<serde_json::Value>(&text).is_ok();
            match serde_json::from_str::<T>(&text) {
                Ok(v) => Loaded {
                    provided: true,
                    loaded: true,
                    malformed: false,
                    value: v,
                },
                Err(_) => Loaded {
                    provided: true,
                    loaded: false,
                    malformed: !is_json,
                    value: T::default(),
                },
            }
        }
    }
}

fn opt_str(p: Option<&Path>) -> Option<String> {
    p.map(|x| x.display().to_string())
}

fn phase(
    name: &str,
    path: Option<&Path>,
    provided: bool,
    loaded: bool,
    ok: Option<bool>,
    message: &str,
) -> CodeWalkerTestSummaryPhase {
    CodeWalkerTestSummaryPhase {
        name: name.to_string(),
        report_path: opt_str(path),
        report_provided: provided,
        report_loaded: loaded,
        ok,
        message: message.to_string(),
    }
}

/// Build a single normalized, read-only summary of a copied-archive CodeWalker
/// test run from whichever pipeline reports were provided.
///
/// This command does NOT run the pipeline. It only reads existing report files
/// and folds them into one verdict plus next-step recommendations. It never calls
/// CodeWalker, never sends an HTTP request (of any method), never executes an
/// external tool, never parses RPF internals, and never modifies any archive or
/// input report. Missing reports yield warnings and an incomplete picture rather
/// than an error; a provided file that is unreadable/malformed yields a warning
/// and is treated as not-loaded. Global `writerAllowed` stays `false`.
#[allow(clippy::too_many_arguments)]
pub fn build_codewalker_test_summary_report(
    compatibility_probe_report_path: Option<&Path>,
    readiness_report_path: Option<&Path>,
    resolve_report_path: Option<&Path>,
    dry_replace_plan_path: Option<&Path>,
    execution_gate_report_path: Option<&Path>,
    replace_apply_report_path: Option<&Path>,
    post_write_verify_report_path: Option<&Path>,
    rollback_restore_report_path: Option<&Path>,
) -> Result<CodeWalkerTestSummaryReport, String> {
    let mut warnings: Vec<CodeWalkerTestSummaryWarning> = Vec::new();

    // ── Read every provided report tolerantly ────────────────────────────────
    let compat = load_view::<CompatProbeView>(compatibility_probe_report_path);
    let readiness = load_view::<ReadinessView>(readiness_report_path);
    let resolve = load_view::<ResolveView>(resolve_report_path);
    let dry = load_view::<DryPlanView>(dry_replace_plan_path);
    let gate = load_view::<ExecutionGateView>(execution_gate_report_path);
    let apply = load_view::<ReplaceApplyView>(replace_apply_report_path);
    let post = load_view::<PostWriteVerifyView>(post_write_verify_report_path);
    let rollback = load_view::<RollbackRestoreView>(rollback_restore_report_path);

    // ── Warnings for provided-but-unusable, and for absent reports ───────────
    let report_specs: [(&str, &Path, bool, bool, bool); 8] = [
        (
            "compatibility_probe_report",
            compatibility_probe_report_path.unwrap_or(Path::new("")),
            compat.provided,
            compat.loaded,
            compat.malformed,
        ),
        (
            "readiness_report",
            readiness_report_path.unwrap_or(Path::new("")),
            readiness.provided,
            readiness.loaded,
            readiness.malformed,
        ),
        (
            "resolve_report",
            resolve_report_path.unwrap_or(Path::new("")),
            resolve.provided,
            resolve.loaded,
            resolve.malformed,
        ),
        (
            "dry_replace_plan",
            dry_replace_plan_path.unwrap_or(Path::new("")),
            dry.provided,
            dry.loaded,
            dry.malformed,
        ),
        (
            "execution_gate_report",
            execution_gate_report_path.unwrap_or(Path::new("")),
            gate.provided,
            gate.loaded,
            gate.malformed,
        ),
        (
            "replace_apply_report",
            replace_apply_report_path.unwrap_or(Path::new("")),
            apply.provided,
            apply.loaded,
            apply.malformed,
        ),
        (
            "post_write_verify_report",
            post_write_verify_report_path.unwrap_or(Path::new("")),
            post.provided,
            post.loaded,
            post.malformed,
        ),
        (
            "rollback_restore_report",
            rollback_restore_report_path.unwrap_or(Path::new("")),
            rollback.provided,
            rollback.loaded,
            rollback.malformed,
        ),
    ];
    let mut reports_provided = 0usize;
    let mut reports_loaded = 0usize;
    for (name, _path, provided, loaded, malformed) in report_specs.iter() {
        if *provided {
            reports_provided += 1;
            if *loaded {
                reports_loaded += 1;
            } else if *malformed {
                warnings.push(CodeWalkerTestSummaryWarning {
                    code: "report_malformed".to_string(),
                    message: format!("Provided '{name}' could not be parsed as JSON; ignored."),
                });
            } else {
                warnings.push(CodeWalkerTestSummaryWarning {
                    code: "report_unusable".to_string(),
                    message: format!(
                        "Provided '{name}' was missing or had an unexpected shape; ignored."
                    ),
                });
            }
        } else {
            warnings.push(CodeWalkerTestSummaryWarning {
                code: "report_absent".to_string(),
                message: format!("No '{name}' was provided; summary is incomplete for that phase."),
            });
        }
    }

    // ── Derive tri-state pipeline facts ──────────────────────────────────────
    // codewalker_reachable: readiness is authoritative; compat root status is a
    // fallback hint.
    let codewalker_reachable = if readiness.loaded {
        readiness.value.codewalker_api_reachable.or(Some(false))
    } else if compat.loaded {
        compat.value.root_http_status.map(|_| true)
    } else {
        None
    };

    let compatibility_probe_ok = if compat.loaded {
        match (
            compat.value.compatible_for_search,
            compat.value.compatible_for_dry_replace_planning,
        ) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), _) | (_, Some(false)) => Some(false),
            _ => None,
        }
    } else {
        None
    };

    let readiness_ok = if readiness.loaded {
        readiness
            .value
            .codewalker_api_ready_for_search
            .or(Some(false))
    } else {
        None
    };

    let targets_resolved = if resolve.loaded {
        let s = &resolve.value.summary;
        Some(s.resolved_count > 0 && s.unresolved_count == 0 && s.ambiguous_count == 0)
    } else {
        None
    };
    let targets_unresolved_or_ambiguous = resolve.loaded
        && (resolve.value.summary.unresolved_count > 0
            || resolve.value.summary.ambiguous_count > 0);

    let dry_plan_valid = if dry.loaded {
        Some(!dry.value.planned_requests.is_empty())
    } else {
        None
    };

    let execution_gate_eligible = if gate.loaded {
        Some(gate.value.codewalker_execution_eligible)
    } else {
        None
    };

    let replace_attempted = if apply.loaded {
        Some(apply.value.replace_requests_sent)
    } else {
        None
    };
    let replace_succeeded = if apply.loaded {
        if apply.value.replace_requests_sent {
            Some(apply.value.successful_replace_count > 0 && apply.value.failed_replace_count == 0)
        } else {
            Some(false)
        }
    } else {
        None
    };

    // target_hash_changed: prefer the apply report's enum, fall back to the
    // post-write verify comparison.
    let target_hash_changed = if apply.loaded {
        match apply.value.target_hash_changed.as_deref() {
            Some("changed") => Some(true),
            Some("unchanged") => Some(false),
            _ => post.value.target_hash_changed_from_pre_apply,
        }
    } else if post.loaded {
        post.value.target_hash_changed_from_pre_apply
    } else {
        None
    };

    let post_write_suspicious = if post.loaded {
        Some(
            post.value
                .verification_result
                .as_deref()
                .map(|r| r.contains("suspicious"))
                .unwrap_or(false),
        )
    } else {
        None
    };
    let post_write_verified = if post.loaded {
        let r = post.value.verification_result.as_deref().unwrap_or("");
        Some(!r.is_empty() && !r.contains("suspicious") && r != "unknown")
    } else {
        None
    };

    let rollback_available = if post.loaded || rollback.loaded {
        Some(post.value.rollback_available || rollback.value.rollback_available)
    } else {
        None
    };
    let rollback_executed = if rollback.loaded {
        Some(
            rollback.value.rollback_executed
                || rollback.value.status.as_deref() == Some("restored"),
        )
    } else if post.loaded {
        Some(post.value.rollback_executed)
    } else {
        None
    };

    // ── Resolve target_rpf / base_url from whatever report carries them ──────
    let target_rpf = gate
        .value
        .target_rpf
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| apply.value.target_rpf.clone().filter(|s| !s.is_empty()))
        .or_else(|| post.value.target_rpf.clone().filter(|s| !s.is_empty()))
        .or_else(|| rollback.value.target_rpf.clone().filter(|s| !s.is_empty()));
    let base_url = readiness
        .value
        .base_url
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| compat.value.base_url.clone().filter(|s| !s.is_empty()))
        .or_else(|| apply.value.base_url.clone().filter(|s| !s.is_empty()));

    // ── Phases ───────────────────────────────────────────────────────────────
    let phases = vec![
        phase(
            "compatibility_probe",
            compatibility_probe_report_path,
            compat.provided,
            compat.loaded,
            compatibility_probe_ok,
            "Was CodeWalker compatible enough for search/replace planning?",
        ),
        phase(
            "readiness",
            readiness_report_path,
            readiness.provided,
            readiness.loaded,
            readiness_ok,
            "Was CodeWalker reachable and ready for search?",
        ),
        phase(
            "resolve_targets",
            resolve_report_path,
            resolve.provided,
            resolve.loaded,
            targets_resolved,
            "Was target resolution successful?",
        ),
        phase(
            "dry_replace_plan",
            dry_replace_plan_path,
            dry.provided,
            dry.loaded,
            dry_plan_valid,
            "Was the dry replace plan valid?",
        ),
        phase(
            "execution_gate",
            execution_gate_report_path,
            gate.provided,
            gate.loaded,
            execution_gate_eligible,
            "Was the execution gate eligible?",
        ),
        phase(
            "replace_apply",
            replace_apply_report_path,
            apply.provided,
            apply.loaded,
            replace_succeeded,
            "Was replace apply attempted and did it succeed?",
        ),
        phase(
            "post_write_verify",
            post_write_verify_report_path,
            post.provided,
            post.loaded,
            post_write_verified,
            "Was post-write verification successful (not suspicious)?",
        ),
        phase(
            "rollback_restore",
            rollback_restore_report_path,
            rollback.provided,
            rollback.loaded,
            rollback_executed,
            "Was rollback executed?",
        ),
    ];

    // ── Final status ─────────────────────────────────────────────────────────
    let final_status = if rollback.loaded && rollback_executed == Some(true) {
        CodeWalkerTestSummaryStatus::RollbackRestored
    } else if post_write_suspicious == Some(true) {
        CodeWalkerTestSummaryStatus::ExecutionSuspicious
    } else if apply.loaded {
        if replace_succeeded == Some(true) && target_hash_changed == Some(true) {
            CodeWalkerTestSummaryStatus::ExecutionSucceededChanged
        } else if replace_succeeded == Some(false) && target_hash_changed == Some(false) {
            CodeWalkerTestSummaryStatus::ExecutionFailedNoChange
        } else if rollback_available == Some(true) && rollback_executed != Some(true) {
            CodeWalkerTestSummaryStatus::RollbackAvailable
        } else {
            CodeWalkerTestSummaryStatus::Unknown
        }
    } else if rollback_available == Some(true) && rollback_executed != Some(true) {
        CodeWalkerTestSummaryStatus::RollbackAvailable
    } else if execution_gate_eligible == Some(true) {
        CodeWalkerTestSummaryStatus::ReadyForExecute
    } else if reports_loaded > 0 {
        CodeWalkerTestSummaryStatus::IncompleteReports
    } else {
        CodeWalkerTestSummaryStatus::NotRun
    };

    // ── Findings ─────────────────────────────────────────────────────────────
    let mut findings: Vec<CodeWalkerTestSummaryFinding> = Vec::new();
    let mut add_finding = |code: &str, sev: CodeWalkerApiSeverity, msg: &str| {
        findings.push(CodeWalkerTestSummaryFinding {
            code: code.to_string(),
            severity: sev,
            message: msg.to_string(),
        });
    };
    match codewalker_reachable {
        Some(true) => add_finding(
            "reachable",
            CodeWalkerApiSeverity::Info,
            "CodeWalker was reachable.",
        ),
        Some(false) => add_finding(
            "offline",
            CodeWalkerApiSeverity::Warning,
            "CodeWalker was not reachable.",
        ),
        None => {}
    }
    if targets_unresolved_or_ambiguous {
        add_finding(
            "targets_unresolved",
            CodeWalkerApiSeverity::Warning,
            "One or more targets were unresolved or ambiguous.",
        );
    }
    if post_write_suspicious == Some(true) {
        add_finding(
            "post_write_suspicious",
            CodeWalkerApiSeverity::Blocking,
            "Post-write verification flagged a suspicious result.",
        );
    }
    if replace_succeeded == Some(true) && target_hash_changed == Some(true) {
        add_finding(
            "replace_succeeded",
            CodeWalkerApiSeverity::Info,
            "Replace apply succeeded and the target hash changed.",
        );
    }
    if replace_attempted == Some(true) && replace_succeeded == Some(false) {
        add_finding(
            "replace_failed",
            CodeWalkerApiSeverity::Warning,
            "Replace apply was attempted but did not succeed.",
        );
    }
    drop(add_finding);

    // ── Recommendations (next safe action) ───────────────────────────────────
    let mut recommendations: Vec<CodeWalkerTestSummaryRecommendation> = Vec::new();
    let mut recommend = |code: &str, msg: &str| {
        recommendations.push(CodeWalkerTestSummaryRecommendation {
            code: code.to_string(),
            message: msg.to_string(),
        });
    };
    match final_status {
        CodeWalkerTestSummaryStatus::RollbackRestored => {
            recommend(
                "proceed_next_real_test",
                "Backup restored. Re-verify the target, then proceed to the next test.",
            );
        }
        CodeWalkerTestSummaryStatus::ExecutionSuspicious => {
            recommend(
                "run_rollback_restore",
                "Result is suspicious — run the rollback restore to recover the backup.",
            );
            recommend(
                "inspect_failed_replace",
                "Inspect the replace apply / verify reports to understand the mismatch.",
            );
        }
        CodeWalkerTestSummaryStatus::ExecutionSucceededChanged => {
            recommend(
                "proceed_next_real_test",
                "Replace succeeded — proceed to the next real test.",
            );
        }
        CodeWalkerTestSummaryStatus::ExecutionFailedNoChange => {
            recommend(
                "inspect_failed_replace",
                "Replace failed with no change — inspect the failed replace response.",
            );
        }
        CodeWalkerTestSummaryStatus::RollbackAvailable => {
            recommend(
                "run_rollback_restore",
                "A rollback plan is available — run rollback restore if recovery is needed.",
            );
        }
        CodeWalkerTestSummaryStatus::ReadyForExecute => {
            recommend(
                "execute_copied_archive_test",
                "Execution gate is eligible — execute the copied archive test.",
            );
        }
        CodeWalkerTestSummaryStatus::NotRun
        | CodeWalkerTestSummaryStatus::IncompleteReports
        | CodeWalkerTestSummaryStatus::Unknown => {
            // Walk the pipeline and recommend the first missing/failing step.
            if codewalker_reachable == Some(false)
                || readiness.loaded && readiness_ok == Some(false)
            {
                recommend(
                    "start_codewalker",
                    "Start CodeWalker.API, then re-run the readiness probe.",
                );
            }
            if !compat.loaded {
                recommend(
                    "run_compatibility_probe",
                    "Run the live compatibility probe.",
                );
            }
            if !readiness.loaded {
                recommend("run_readiness_probe", "Run the readiness probe.");
            }
            if !resolve.loaded {
                recommend("run_resolve_targets", "Run resolve targets.");
            } else if targets_unresolved_or_ambiguous {
                recommend(
                    "fix_targets",
                    "Fix the ambiguous/unresolved targets, then re-resolve.",
                );
            }
            if !dry.loaded {
                recommend("run_dry_replace_plan", "Run the dry replace plan.");
            }
            if !gate.loaded {
                recommend("run_execution_gate", "Run the execution gate.");
            }
        }
    }
    drop(recommend);
    // Offline always warrants a start recommendation regardless of status.
    if codewalker_reachable == Some(false)
        && !recommendations.iter().any(|r| r.code == "start_codewalker")
    {
        recommendations.push(CodeWalkerTestSummaryRecommendation {
            code: "start_codewalker".to_string(),
            message: "Start CodeWalker.API, then re-run the readiness probe.".to_string(),
        });
    }
    // Unresolved targets always warrant a fix recommendation.
    if targets_unresolved_or_ambiguous && !recommendations.iter().any(|r| r.code == "fix_targets") {
        recommendations.push(CodeWalkerTestSummaryRecommendation {
            code: "fix_targets".to_string(),
            message: "Fix the ambiguous/unresolved targets, then re-resolve.".to_string(),
        });
    }

    // ── Standing blocked items (read-only facts) ─────────────────────────────
    let blocked_items = vec![
        CodeWalkerTestSummaryBlockedItem {
            component: "summary".to_string(),
            reason: "This command is read-only; it never runs the pipeline or modifies archives."
                .to_string(),
            block_type: "summary_is_read_only".to_string(),
        },
        CodeWalkerTestSummaryBlockedItem {
            component: "parser".to_string(),
            reason: "Native RPF parsing is not implemented.".to_string(),
            block_type: "native_rpf_parser_not_implemented".to_string(),
        },
        CodeWalkerTestSummaryBlockedItem {
            component: "writer".to_string(),
            reason: "Global RPF writing remains disabled.".to_string(),
            block_type: "global_writer_disabled".to_string(),
        },
    ];

    let summary = CodeWalkerTestSummarySummary {
        reports_provided,
        reports_loaded,
        phase_count: phases.len(),
        finding_count: findings.len(),
        warning_count: warnings.len(),
        blocked_count: blocked_items.len(),
        recommendation_count: recommendations.len(),
        final_status,
    };

    Ok(CodeWalkerTestSummaryReport {
        final_status,
        target_rpf,
        base_url,
        compatibility_probe_report_path: opt_str(compatibility_probe_report_path),
        readiness_report_path: opt_str(readiness_report_path),
        resolve_report_path: opt_str(resolve_report_path),
        dry_replace_plan_path: opt_str(dry_replace_plan_path),
        execution_gate_report_path: opt_str(execution_gate_report_path),
        replace_apply_report_path: opt_str(replace_apply_report_path),
        post_write_verify_report_path: opt_str(post_write_verify_report_path),
        rollback_restore_report_path: opt_str(rollback_restore_report_path),
        phases,
        findings,
        warnings,
        blocked_items,
        recommendations,
        codewalker_reachable,
        compatibility_probe_ok,
        readiness_ok,
        targets_resolved,
        dry_plan_valid,
        execution_gate_eligible,
        replace_attempted,
        replace_succeeded,
        target_hash_changed,
        post_write_verified,
        post_write_suspicious,
        rollback_available,
        rollback_executed,
        no_http_requests_sent_by_summary: true,
        no_archive_modified_by_summary: true,
        native_parser_used: false,
        external_tool_executed: false,
        writer_allowed_global: false,
        summary,
    })
}
