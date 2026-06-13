#[cfg(test)]
mod test_summary_tests {
    use crate::codewalker_api::model::CodeWalkerTestSummaryStatus;
    use crate::codewalker_api::test_summary::build_codewalker_test_summary_report;
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};

    // ── Fake-report helpers (no real CodeWalker, no HTTP, no GTA files) ──────

    fn tmp_dir() -> PathBuf {
        let mut d = std::env::temp_dir();
        let unique = format!(
            "cw_test_summary_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        d.push(unique);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_json(dir: &Path, name: &str, value: &Value) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, serde_json::to_string_pretty(value).unwrap()).unwrap();
        p
    }

    fn compat(ok: bool) -> Value {
        json!({
            "baseUrl": "http://localhost:5555",
            "rootHttpStatus": 200,
            "compatibleForSearch": ok,
            "compatibleForDryReplacePlanning": ok
        })
    }

    fn readiness(reachable: bool, ready: bool) -> Value {
        json!({
            "baseUrl": "http://localhost:5555",
            "codewalkerApiReachable": reachable,
            "codewalkerApiReadyForSearch": ready
        })
    }

    fn resolve(resolved: u64, unresolved: u64, ambiguous: u64) -> Value {
        json!({
            "codewalkerApiReachable": true,
            "summary": {
                "targetCount": resolved + unresolved + ambiguous,
                "resolvedCount": resolved,
                "unresolvedCount": unresolved,
                "ambiguousCount": ambiguous
            }
        })
    }

    fn dry_plan(requests: usize) -> Value {
        let reqs: Vec<Value> = (0..requests).map(|i| json!({ "index": i })).collect();
        json!({ "plannedRequests": reqs })
    }

    fn gate(eligible: bool) -> Value {
        json!({
            "targetRpf": "C:/tmp/copied_test/update_test_copy.rpf",
            "codewalkerExecutionEligible": eligible
        })
    }

    fn apply(sent: bool, success: u64, failed: u64, hash: &str) -> Value {
        json!({
            "targetRpf": "C:/tmp/copied_test/update_test_copy.rpf",
            "baseUrl": "http://localhost:5555",
            "replaceRequestsSent": sent,
            "successfulReplaceCount": success,
            "failedReplaceCount": failed,
            "targetHashChanged": hash
        })
    }

    fn post(result: &str, changed: Option<bool>, rollback_available: bool) -> Value {
        json!({
            "targetRpf": "C:/tmp/copied_test/update_test_copy.rpf",
            "verificationResult": result,
            "targetHashChangedFromPreApply": changed,
            "rollbackAvailable": rollback_available,
            "rollbackExecuted": false
        })
    }

    fn rollback(status: &str, executed: bool) -> Value {
        json!({
            "targetRpf": "C:/tmp/copied_test/update_test_copy.rpf",
            "status": status,
            "rollbackAvailable": true,
            "rollbackExecuted": executed,
            "restoredTargetMatchesBackup": executed
        })
    }

    fn file_bytes(p: &Path) -> Vec<u8> {
        std::fs::read(p).unwrap()
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[test]
    fn codewalker_test_summary_handles_no_reports() {
        let r =
            build_codewalker_test_summary_report(None, None, None, None, None, None, None, None)
                .unwrap();
        assert_eq!(r.final_status, CodeWalkerTestSummaryStatus::NotRun);
        assert_eq!(r.summary.reports_provided, 0);
        assert_eq!(r.summary.reports_loaded, 0);
        assert!(r.no_http_requests_sent_by_summary);
        assert!(r.no_archive_modified_by_summary);
        assert!(!r.native_parser_used);
        assert!(!r.external_tool_executed);
        assert!(!r.writer_allowed_global);
        // Every absent report yields a warning.
        assert_eq!(r.warnings.len(), 8);
    }

    #[test]
    fn codewalker_test_summary_reads_compatibility_probe() {
        let d = tmp_dir();
        let p = write_json(&d, "compat.json", &compat(true));
        let r = build_codewalker_test_summary_report(
            Some(&p),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.compatibility_probe_ok, Some(true));
        assert_eq!(r.summary.reports_loaded, 1);
    }

    #[test]
    fn codewalker_test_summary_reads_readiness_report() {
        let d = tmp_dir();
        let p = write_json(&d, "readiness.json", &readiness(true, true));
        let r = build_codewalker_test_summary_report(
            None,
            Some(&p),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.codewalker_reachable, Some(true));
        assert_eq!(r.readiness_ok, Some(true));
    }

    #[test]
    fn codewalker_test_summary_reads_resolve_report() {
        let d = tmp_dir();
        let p = write_json(&d, "resolve.json", &resolve(2, 0, 0));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            Some(&p),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.targets_resolved, Some(true));
    }

    #[test]
    fn codewalker_test_summary_reads_dry_replace_plan() {
        let d = tmp_dir();
        let p = write_json(&d, "dry.json", &dry_plan(3));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            Some(&p),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.dry_plan_valid, Some(true));
    }

    #[test]
    fn codewalker_test_summary_reads_execution_gate() {
        let d = tmp_dir();
        let p = write_json(&d, "gate.json", &gate(true));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            Some(&p),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.execution_gate_eligible, Some(true));
        assert_eq!(
            r.target_rpf.as_deref(),
            Some("C:/tmp/copied_test/update_test_copy.rpf")
        );
    }

    #[test]
    fn codewalker_test_summary_reads_replace_apply_report() {
        let d = tmp_dir();
        let p = write_json(&d, "apply.json", &apply(true, 1, 0, "changed"));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            Some(&p),
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.replace_attempted, Some(true));
        assert_eq!(r.replace_succeeded, Some(true));
        assert_eq!(r.target_hash_changed, Some(true));
    }

    #[test]
    fn codewalker_test_summary_reads_post_write_verify_report() {
        let d = tmp_dir();
        let p = write_json(
            &d,
            "post.json",
            &post("execution_succeeded_target_changed", Some(true), true),
        );
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&p),
            None,
        )
        .unwrap();
        assert_eq!(r.post_write_verified, Some(true));
        assert_eq!(r.post_write_suspicious, Some(false));
        assert_eq!(r.rollback_available, Some(true));
    }

    #[test]
    fn codewalker_test_summary_reads_rollback_restore_report() {
        let d = tmp_dir();
        let p = write_json(&d, "rollback.json", &rollback("restored", true));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&p),
        )
        .unwrap();
        assert_eq!(r.rollback_executed, Some(true));
    }

    #[test]
    fn codewalker_test_summary_status_ready_for_execute_when_gate_eligible() {
        let d = tmp_dir();
        let g = write_json(&d, "gate.json", &gate(true));
        let dp = write_json(&d, "dry.json", &dry_plan(1));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            Some(&dp),
            Some(&g),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.final_status, CodeWalkerTestSummaryStatus::ReadyForExecute);
        assert!(r
            .recommendations
            .iter()
            .any(|x| x.code == "execute_copied_archive_test"));
    }

    #[test]
    fn codewalker_test_summary_status_failed_no_change() {
        let d = tmp_dir();
        let a = write_json(&d, "apply.json", &apply(true, 0, 1, "unchanged"));
        let p = write_json(
            &d,
            "post.json",
            &post("execution_failed_no_change", Some(false), false),
        );
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            Some(&a),
            Some(&p),
            None,
        )
        .unwrap();
        assert_eq!(
            r.final_status,
            CodeWalkerTestSummaryStatus::ExecutionFailedNoChange
        );
        assert!(r
            .recommendations
            .iter()
            .any(|x| x.code == "inspect_failed_replace"));
    }

    #[test]
    fn codewalker_test_summary_status_succeeded_changed() {
        let d = tmp_dir();
        let a = write_json(&d, "apply.json", &apply(true, 1, 0, "changed"));
        let p = write_json(
            &d,
            "post.json",
            &post("execution_succeeded_target_changed", Some(true), true),
        );
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            Some(&a),
            Some(&p),
            None,
        )
        .unwrap();
        assert_eq!(
            r.final_status,
            CodeWalkerTestSummaryStatus::ExecutionSucceededChanged
        );
        assert!(r
            .recommendations
            .iter()
            .any(|x| x.code == "proceed_next_real_test"));
    }

    #[test]
    fn codewalker_test_summary_status_suspicious() {
        let d = tmp_dir();
        let a = write_json(&d, "apply.json", &apply(true, 0, 1, "changed"));
        let p = write_json(
            &d,
            "post.json",
            &post(
                "execution_failed_but_target_changed_suspicious",
                Some(true),
                true,
            ),
        );
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            Some(&a),
            Some(&p),
            None,
        )
        .unwrap();
        assert_eq!(
            r.final_status,
            CodeWalkerTestSummaryStatus::ExecutionSuspicious
        );
    }

    #[test]
    fn codewalker_test_summary_status_rollback_available() {
        let d = tmp_dir();
        let p = write_json(
            &d,
            "post.json",
            &post("no_execution_no_change", Some(false), true),
        );
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&p),
            None,
        )
        .unwrap();
        assert_eq!(
            r.final_status,
            CodeWalkerTestSummaryStatus::RollbackAvailable
        );
    }

    #[test]
    fn codewalker_test_summary_status_rollback_restored() {
        let d = tmp_dir();
        let rb = write_json(&d, "rollback.json", &rollback("restored", true));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&rb),
        )
        .unwrap();
        assert_eq!(
            r.final_status,
            CodeWalkerTestSummaryStatus::RollbackRestored
        );
        assert!(r
            .recommendations
            .iter()
            .any(|x| x.code == "proceed_next_real_test"));
    }

    #[test]
    fn codewalker_test_summary_recommends_rollback_when_suspicious() {
        let d = tmp_dir();
        let a = write_json(&d, "apply.json", &apply(true, 1, 0, "unchanged"));
        let p = write_json(
            &d,
            "post.json",
            &post(
                "execution_succeeded_but_target_unchanged_suspicious",
                Some(false),
                true,
            ),
        );
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            None,
            Some(&a),
            Some(&p),
            None,
        )
        .unwrap();
        assert_eq!(
            r.final_status,
            CodeWalkerTestSummaryStatus::ExecutionSuspicious
        );
        assert!(r
            .recommendations
            .iter()
            .any(|x| x.code == "run_rollback_restore"));
    }

    #[test]
    fn codewalker_test_summary_recommends_start_codewalker_when_offline() {
        let d = tmp_dir();
        let p = write_json(&d, "readiness.json", &readiness(false, false));
        let r = build_codewalker_test_summary_report(
            None,
            Some(&p),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.codewalker_reachable, Some(false));
        assert!(r
            .recommendations
            .iter()
            .any(|x| x.code == "start_codewalker"));
    }

    #[test]
    fn codewalker_test_summary_recommends_fix_targets_when_unresolved() {
        let d = tmp_dir();
        let p = write_json(&d, "resolve.json", &resolve(1, 2, 0));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            Some(&p),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.targets_resolved, Some(false));
        assert!(r.recommendations.iter().any(|x| x.code == "fix_targets"));
    }

    #[test]
    fn codewalker_test_summary_out_file_written_when_requested() {
        let d = tmp_dir();
        let g = write_json(&d, "gate.json", &gate(true));
        let r = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            Some(&g),
            None,
            None,
            None,
        )
        .unwrap();

        // Simulate the CLI --out behavior: the summary is only written when an
        // output path is requested.
        let out = d.join("summary_out.json");
        assert!(!out.exists());
        std::fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.exists());
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(written["finalStatus"], json!("ready_for_execute"));
    }

    #[test]
    fn codewalker_test_summary_does_not_modify_files() {
        let d = tmp_dir();
        let g = write_json(&d, "gate.json", &gate(true));
        let a = write_json(&d, "apply.json", &apply(true, 1, 0, "changed"));
        let p = write_json(
            &d,
            "post.json",
            &post("execution_succeeded_target_changed", Some(true), true),
        );

        let before_g = file_bytes(&g);
        let before_a = file_bytes(&a);
        let before_p = file_bytes(&p);

        let _ = build_codewalker_test_summary_report(
            None,
            None,
            None,
            None,
            Some(&g),
            Some(&a),
            Some(&p),
            None,
        )
        .unwrap();

        // Inputs are byte-identical after summarizing.
        assert_eq!(before_g, file_bytes(&g));
        assert_eq!(before_a, file_bytes(&a));
        assert_eq!(before_p, file_bytes(&p));

        // No output file was created (none was requested).
        assert!(!d.join("summary_out.json").exists());
    }
}
