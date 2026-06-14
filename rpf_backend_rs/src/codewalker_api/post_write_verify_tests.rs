#[cfg(test)]
mod post_write_verify_tests {
    use crate::codewalker_api::model::{
        CodeWalkerPostWriteResult, CodeWalkerPostWriteVerifyStatus, CodeWalkerRollbackPlanStatus,
    };
    use crate::codewalker_api::post_write_verify::build_codewalker_post_write_verify_report;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};

    const RPF_CONTENT: &[u8] = b"FAKE-RPF copied test archive fixture\n";
    const OTHER_HASH: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        format!("{:x}", h.finalize())
    }

    fn write_target(dir: &Path) -> PathBuf {
        let p = dir.join("test_copies/fake_update.rpf");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, RPF_CONTENT).unwrap();
        p
    }

    fn write_json(dir: &Path, name: &str, v: &Value) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
        p
    }

    /// Replace apply report with controllable execution/hash facts.
    fn apply_report(dir: &Path, sent: bool, success: u64, failed: u64, pre_hash: &str) -> PathBuf {
        write_json(
            dir,
            "codewalker_replace_apply.json",
            &json!({
                "status": if sent { if success > 0 { "executed" } else { "failed" } } else { "blocked" },
                "replaceRequestsSent": sent,
                "successfulReplaceCount": success,
                "failedReplaceCount": failed,
                "originalTargetSha256": pre_hash,
                "postExecutionTargetSha256": pre_hash
            }),
        )
    }

    /// A valid backup report whose backup file exists and target matches.
    fn backup_report(dir: &Path, target: &Path, valid: bool) -> PathBuf {
        let backup_file = dir.join("backups/fake_update.rpf.bak");
        fs::create_dir_all(backup_file.parent().unwrap()).unwrap();
        fs::write(&backup_file, RPF_CONTENT).unwrap();
        write_json(
            dir,
            "rpf_backup_report.json",
            &json!({
                "status": "backed_up",
                "targetArchivePath": target.display().to_string(),
                "backupFilePath": backup_file.display().to_string(),
                "originalHash": sha256_hex(RPF_CONTENT),
                "backupHash": sha256_hex(RPF_CONTENT),
                "hashVerified": valid,
                "safeForFutureWrite": valid
            }),
        )
    }

    fn gate_report(dir: &Path, target: &Path) -> PathBuf {
        write_json(
            dir,
            "codewalker_execution_gate.json",
            &json!({
                "codewalkerExecutionEligible": true,
                "targetArchiveClassification": "copied_test_archive",
                "targetRpf": target.display().to_string()
            }),
        )
    }

    fn dry_plan(dir: &Path) -> PathBuf {
        write_json(
            dir,
            "codewalker_dry_replace_plan.json",
            &json!({
                "dryRunOnly": true,
                "plannedRequests": [{ "endpoint": "/api/replace-file", "method": "POST" }]
            }),
        )
    }

    struct Setup {
        target: PathBuf,
        apply: PathBuf,
        backup: PathBuf,
        gate: PathBuf,
        plan: PathBuf,
    }

    /// Build a setup. `sent`/`success`/`failed`/`pre_hash` drive the result; the
    /// current target hash is always `sha256(RPF_CONTENT)`.
    fn setup(dir: &Path, sent: bool, success: u64, failed: u64, pre_hash: &str) -> Setup {
        let target = write_target(dir);
        let apply = apply_report(dir, sent, success, failed, pre_hash);
        let backup = backup_report(dir, &target, true);
        let gate = gate_report(dir, &target);
        let plan = dry_plan(dir);
        Setup {
            target,
            apply,
            backup,
            gate,
            plan,
        }
    }

    fn run(s: &Setup) -> crate::codewalker_api::model::CodeWalkerPostWriteVerifyReport {
        build_codewalker_post_write_verify_report(&s.target, &s.apply, &s.backup, &s.gate, &s.plan)
            .unwrap()
    }

    fn current_hash() -> String {
        sha256_hex(RPF_CONTENT)
    }

    // ── Report reading ──────────────────────────────────────────────────────

    #[test]
    fn post_write_verify_still_reports_failed_no_change_on_http_400() {
        // Replace sent but failed (HTTP 400) with the target unchanged — the
        // T0.6.14/T0.6.15 live shape. Must classify as execution_failed_no_change.
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 0, 1, &current_hash());
        let r = run(&s);
        assert_eq!(r.status, CodeWalkerPostWriteVerifyStatus::Verified);
        assert_eq!(
            r.verification_result,
            CodeWalkerPostWriteResult::ExecutionFailedNoChange
        );
        assert!(!r.modifies_archive);
        // A backup-based rollback plan is still available but not needed.
        assert_eq!(
            r.rollback_plan.rollback_plan_status,
            CodeWalkerRollbackPlanStatus::Ready
        );
        assert!(!r.rollback_executed);
    }

    #[test]
    fn codewalker_post_write_verify_reads_replace_apply_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, &current_hash());
        let r = run(&s);
        assert!(r.replace_requests_sent);
        assert_eq!(r.successful_replace_count, 1);
        assert_eq!(r.replace_apply_status.as_deref(), Some("executed"));
    }

    #[test]
    fn codewalker_post_write_verify_reads_backup_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, &current_hash());
        let r = run(&s);
        assert!(r.backup_hash_verified);
        assert!(r.backup_safe_for_future_write);
        assert!(r.backup_file_exists);
    }

    #[test]
    fn codewalker_post_write_verify_reads_execution_gate_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, &current_hash());
        let r = run(&s);
        assert!(r.execution_gate_was_eligible);
        assert!(r.copied_test_archive_confirmed);
    }

    #[test]
    fn codewalker_post_write_verify_reads_dry_replace_plan() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, &current_hash());
        let r = run(&s);
        assert_eq!(r.dry_plan_planned_request_count, 1);
    }

    #[test]
    fn codewalker_post_write_verify_computes_current_target_hash() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, &current_hash());
        let r = run(&s);
        assert_eq!(
            r.target_current_sha256.as_deref(),
            Some(current_hash().as_str())
        );
        assert_eq!(r.target_current_size_bytes, Some(RPF_CONTENT.len() as u64));
        assert_eq!(r.status, CodeWalkerPostWriteVerifyStatus::Verified);
    }

    // ── Verification result classification ──────────────────────────────────

    #[test]
    fn codewalker_post_write_verify_detects_no_execution_no_change() {
        let dir = tempfile::TempDir::new().unwrap();
        // Not sent, pre == current -> unchanged.
        let s = setup(dir.path(), false, 0, 0, &current_hash());
        let r = run(&s);
        assert_eq!(
            r.verification_result,
            CodeWalkerPostWriteResult::NoExecutionNoChange
        );
    }

    #[test]
    fn codewalker_post_write_verify_detects_failed_no_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 0, 1, &current_hash());
        let r = run(&s);
        assert_eq!(
            r.verification_result,
            CodeWalkerPostWriteResult::ExecutionFailedNoChange
        );
    }

    #[test]
    fn codewalker_post_write_verify_detects_failed_but_changed_suspicious() {
        let dir = tempfile::TempDir::new().unwrap();
        // Failed, pre != current -> target changed despite failure.
        let s = setup(dir.path(), true, 0, 1, OTHER_HASH);
        let r = run(&s);
        assert_eq!(
            r.verification_result,
            CodeWalkerPostWriteResult::ExecutionFailedButTargetChangedSuspicious
        );
        assert!(r.rollback_recommended);
    }

    #[test]
    fn codewalker_post_write_verify_detects_success_changed() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert_eq!(
            r.verification_result,
            CodeWalkerPostWriteResult::ExecutionSucceededTargetChanged
        );
    }

    #[test]
    fn codewalker_post_write_verify_detects_success_unchanged_suspicious() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, &current_hash());
        let r = run(&s);
        assert_eq!(
            r.verification_result,
            CodeWalkerPostWriteResult::ExecutionSucceededButTargetUnchangedSuspicious
        );
        assert!(r.rollback_recommended);
    }

    // ── Rollback plan ───────────────────────────────────────────────────────

    #[test]
    fn codewalker_post_write_verify_builds_rollback_plan_when_backup_valid() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(r.rollback_available);
        assert_eq!(
            r.rollback_plan.rollback_plan_status,
            CodeWalkerRollbackPlanStatus::Ready
        );
        assert_eq!(
            r.rollback_plan.restore_method_planned,
            "copy_backup_over_target"
        );
        assert!(r.rollback_plan.backup_file_path.is_some());
    }

    #[test]
    fn codewalker_post_write_verify_blocks_or_warns_when_backup_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = write_target(dir.path());
        let apply = apply_report(dir.path(), true, 1, 0, OTHER_HASH);
        // Backup report references a backup file that does not exist.
        let backup = write_json(
            dir.path(),
            "rpf_backup_report.json",
            &json!({
                "targetArchivePath": target.display().to_string(),
                "backupFilePath": dir.path().join("backups/missing.bak").display().to_string(),
                "hashVerified": true,
                "safeForFutureWrite": true
            }),
        );
        let gate = gate_report(dir.path(), &target);
        let plan = dry_plan(dir.path());
        let r = build_codewalker_post_write_verify_report(&target, &apply, &backup, &gate, &plan)
            .unwrap();
        assert!(!r.backup_file_exists);
        assert!(!r.rollback_available);
        assert_eq!(
            r.rollback_plan.rollback_plan_status,
            CodeWalkerRollbackPlanStatus::Unavailable
        );
        assert!(r.warnings.iter().any(|w| w.code == "rollback_unavailable"));
    }

    #[test]
    fn codewalker_post_write_verify_requires_backup_hash_verified_for_rollback_available() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = write_target(dir.path());
        let apply = apply_report(dir.path(), true, 1, 0, OTHER_HASH);
        let backup = backup_report(dir.path(), &target, false); // hashVerified=false
        let gate = gate_report(dir.path(), &target);
        let plan = dry_plan(dir.path());
        let r = build_codewalker_post_write_verify_report(&target, &apply, &backup, &gate, &plan)
            .unwrap();
        assert!(!r.backup_hash_verified);
        assert!(!r.rollback_available);
    }

    #[test]
    fn codewalker_post_write_verify_rollback_executed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.rollback_executed);
        assert!(!r.rollback_plan.rollback_executed);
        assert!(!r.summary.rollback_executed);
    }

    #[test]
    fn codewalker_post_write_verify_rollback_execution_allowed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.rollback_execution_allowed);
        assert!(!r.rollback_plan.rollback_execution_supported);
        assert!(!r.rollback_plan.safe_to_execute_now);
    }

    // ── Safety: no network / no mutation ────────────────────────────────────

    #[test]
    fn codewalker_post_write_verify_does_not_send_http_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.http_requests_sent);
    }

    #[test]
    fn codewalker_post_write_verify_does_not_use_post() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.post_requests_sent);
    }

    #[test]
    fn codewalker_post_write_verify_does_not_call_replace_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.replace_endpoint_called);
    }

    #[test]
    fn codewalker_post_write_verify_does_not_call_import_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.import_endpoint_called);
    }

    #[test]
    fn codewalker_post_write_verify_does_not_call_reload_services() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.reload_services_called);
    }

    #[test]
    fn codewalker_post_write_verify_does_not_call_set_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.set_config_called);
    }

    #[test]
    fn codewalker_post_write_verify_does_not_execute_external_tool() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        assert!(!r.external_tool_executed);
        assert!(!r.native_parser_used);
        assert!(!r.native_writer_used);
    }

    #[test]
    fn codewalker_post_write_verify_does_not_modify_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let before = fs::read(&s.target).unwrap();
        let _ = run(&s);
        let after = fs::read(&s.target).unwrap();
        assert_eq!(before, after);
        assert_eq!(after, RPF_CONTENT);
    }

    #[test]
    fn codewalker_post_write_verify_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path(), true, 1, 0, OTHER_HASH);
        let r = run(&s);
        let out = dir.path().join("codewalker_post_write_verify.json");
        fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["rollbackExecuted"], false);
        assert_eq!(v["rollbackExecutionAllowed"], false);
        assert_eq!(v["modifiesArchive"], false);
        assert_eq!(v["httpRequestsSent"], false);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
        assert_eq!(
            v["rollbackPlan"]["restoreMethodPlanned"],
            "copy_backup_over_target"
        );
    }
}
