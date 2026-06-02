#[cfg(test)]
mod execution_gate_tests {
    use crate::codewalker_api::execution_gate::build_codewalker_execution_gate_report;
    use crate::codewalker_api::model::{
        CodeWalkerExecutionGateStatus, CodeWalkerTargetArchiveClassification,
    };
    use serde_json::{json, Value};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A tiny fake `.rpf` fixture — never a real archive, never parsed.
    const RPF_CONTENT: &[u8] = b"FAKE-RPF copied test archive fixture\n";

    fn write_target(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, RPF_CONTENT).unwrap();
        p
    }

    fn write_json(dir: &Path, name: &str, v: &Value) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
        p
    }

    fn dry_plan_ok(dir: &Path) -> PathBuf {
        write_json(
            dir,
            "codewalker_dry_replace_plan.json",
            &json!({
                "status": "planned",
                "dryRunOnly": true,
                "readyForExecution": false,
                "writerAllowed": false,
                "replaceEndpointCalled": false,
                "postRequestsSent": false,
                "plannedRequests": [{
                    "endpoint": "/api/replace-file",
                    "method": "POST",
                    "dryRunOnly": true
                }]
            }),
        )
    }

    fn permission_ok(dir: &Path) -> PathBuf {
        write_json(
            dir,
            "writer_permission_report.json",
            &json!({
                "status": "token_issued",
                "confirmationPhraseMatched": true,
                "writerAllowed": false,
                "permissionToken": { "tokenId": "abc", "tokenVersion": "1" }
            }),
        )
    }

    fn readiness_ok(dir: &Path) -> PathBuf {
        write_json(
            dir,
            "write_readiness_report.json",
            &json!({ "status": "not_ready", "readyToWrite": false }),
        )
    }

    fn manifest_ok(dir: &Path) -> PathBuf {
        write_json(
            dir,
            "rpf_entry_manifest_report.json",
            &json!({
                "status": "built",
                "readyForWrite": false,
                "manifest": { "entries": [{ "archiveRelativePath": "common/data/x.dat" }] }
            }),
        )
    }

    fn backup_ok(dir: &Path, target: &Path) -> PathBuf {
        write_json(
            dir,
            "rpf_backup_report.json",
            &json!({
                "status": "verified",
                "targetArchivePath": target.display().to_string(),
                "hashVerified": true,
                "safeForFutureWrite": true
            }),
        )
    }

    /// Full happy path: copied test archive + all five valid reports.
    struct Happy {
        target: PathBuf,
        dry: PathBuf,
        perm: PathBuf,
        ready: PathBuf,
        manifest: PathBuf,
        backup: PathBuf,
    }

    fn happy(dir: &Path) -> Happy {
        let target = write_target(dir, "test_copies/fake_update.rpf");
        let dry = dry_plan_ok(dir);
        let perm = permission_ok(dir);
        let ready = readiness_ok(dir);
        let manifest = manifest_ok(dir);
        let backup = backup_ok(dir, &target);
        Happy {
            target,
            dry,
            perm,
            ready,
            manifest,
            backup,
        }
    }

    fn run(
        h: &Happy,
        target_is_test_copy: bool,
    ) -> crate::codewalker_api::model::CodeWalkerExecutionGateReport {
        build_codewalker_execution_gate_report(
            &h.target,
            &h.dry,
            &h.perm,
            &h.ready,
            &h.manifest,
            &h.backup,
            target_is_test_copy,
        )
        .unwrap()
    }

    #[test]
    fn codewalker_execution_gate_requires_target_rpf() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut h = happy(dir.path());
        h.target = dir.path().join("test_copies/does_not_exist.rpf");
        let r = run(&h, true);
        assert!(!r.target_rpf_exists);
        assert!(!r.codewalker_execution_eligible);
    }

    #[test]
    fn codewalker_execution_gate_blocks_non_rpf_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut h = happy(dir.path());
        h.target = write_target(dir.path(), "test_copies/fake_update.bin");
        let r = run(&h, true);
        assert!(!r.target_rpf_extension_valid);
        assert_eq!(
            r.target_archive_classification,
            CodeWalkerTargetArchiveClassification::InvalidExtension
        );
        assert!(!r.codewalker_execution_eligible);
    }

    #[test]
    fn codewalker_execution_gate_requires_test_copy_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, false);
        assert_eq!(
            r.target_archive_classification,
            CodeWalkerTargetArchiveClassification::UnknownArchive
        );
        assert!(!r.codewalker_execution_eligible);
    }

    #[test]
    fn codewalker_execution_gate_blocks_original_game_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut h = happy(dir.path());
        // A copied report stack, but the path looks like an original install.
        h.target = write_target(
            dir.path(),
            "steamapps/common/Grand Theft Auto V/update/update.rpf",
        );
        h.backup = backup_ok(dir.path(), &h.target);
        let r = run(&h, true);
        assert_eq!(
            r.target_archive_classification,
            CodeWalkerTargetArchiveClassification::OriginalGameArchiveSuspected
        );
        assert!(!r.target_path_allowed_for_test_execution);
        assert!(!r.codewalker_execution_eligible);
    }

    #[test]
    fn codewalker_execution_gate_accepts_copied_test_archive_fixture() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert_eq!(
            r.target_archive_classification,
            CodeWalkerTargetArchiveClassification::CopiedTestArchive
        );
        assert!(r.target_path_allowed_for_test_execution);
    }

    #[test]
    fn codewalker_execution_gate_reads_dry_replace_plan() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(r.dry_replace_plan_valid);
        assert!(r.dry_plan_has_planned_requests);
        assert!(!r.dry_plan_ready_for_execution);
    }

    #[test]
    fn codewalker_execution_gate_requires_planned_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut h = happy(dir.path());
        h.dry = write_json(
            dir.path(),
            "codewalker_dry_replace_plan.json",
            &json!({
                "dryRunOnly": true,
                "replaceEndpointCalled": false,
                "postRequestsSent": false,
                "plannedRequests": []
            }),
        );
        let r = run(&h, true);
        assert!(!r.dry_plan_has_planned_requests);
        assert!(!r.codewalker_execution_eligible);
    }

    #[test]
    fn codewalker_execution_gate_reads_permission_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(r.permission_report_valid);
    }

    #[test]
    fn codewalker_execution_gate_requires_permission_token() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut h = happy(dir.path());
        h.perm = write_json(
            dir.path(),
            "writer_permission_report.json",
            &json!({ "confirmationPhraseMatched": true, "writerAllowed": false }),
        );
        let r = run(&h, true);
        assert!(!r.permission_token_present);
        assert!(!r.codewalker_execution_eligible);
    }

    #[test]
    fn codewalker_execution_gate_reads_readiness_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(r.readiness_report_valid);
    }

    #[test]
    fn codewalker_execution_gate_reads_entry_manifest_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(r.entry_manifest_report_valid);
    }

    #[test]
    fn codewalker_execution_gate_reads_backup_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(r.backup_report_valid);
    }

    #[test]
    fn codewalker_execution_gate_requires_backup_hash_verified() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut h = happy(dir.path());
        h.backup = write_json(
            dir.path(),
            "rpf_backup_report.json",
            &json!({
                "targetArchivePath": h.target.display().to_string(),
                "hashVerified": false,
                "safeForFutureWrite": true
            }),
        );
        let r = run(&h, true);
        assert!(!r.backup_hash_verified);
        assert!(!r.codewalker_execution_eligible);
    }

    #[test]
    fn codewalker_execution_gate_requires_backup_target_match() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut h = happy(dir.path());
        h.backup = write_json(
            dir.path(),
            "rpf_backup_report.json",
            &json!({
                "targetArchivePath": "some/other/archive.rpf",
                "hashVerified": true,
                "safeForFutureWrite": true
            }),
        );
        let r = run(&h, true);
        assert!(!r.backup_report_valid);
        assert!(!r.codewalker_execution_eligible);
        let g = r
            .gates
            .iter()
            .find(|g| g.name == "backup_target_matches_execution_target")
            .unwrap();
        assert!(!g.passed);
    }

    #[test]
    fn codewalker_execution_gate_eligible_true_when_all_strict_gates_pass() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert_eq!(r.status, CodeWalkerExecutionGateStatus::Eligible);
        assert!(r.codewalker_execution_eligible);
        assert!(r.summary.strict_gates_all_passed);
    }

    #[test]
    fn codewalker_execution_gate_allowed_now_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.codewalker_execution_allowed_now);
        assert!(!r.summary.codewalker_execution_allowed_now);
    }

    #[test]
    fn codewalker_execution_gate_execution_performed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.codewalker_execution_performed);
        assert!(!r.summary.codewalker_execution_performed);
        assert!(!r.modifies_archive);
    }

    #[test]
    fn codewalker_execution_gate_writer_allowed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.writer_allowed);
        assert!(!r.summary.writer_allowed);
    }

    #[test]
    fn codewalker_execution_gate_does_not_send_http_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.http_requests_sent);
    }

    #[test]
    fn codewalker_execution_gate_does_not_use_post() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.post_requests_sent);
    }

    #[test]
    fn codewalker_execution_gate_does_not_call_replace_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.replace_endpoint_called);
    }

    #[test]
    fn codewalker_execution_gate_does_not_call_import_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.import_endpoint_called);
    }

    #[test]
    fn codewalker_execution_gate_does_not_call_reload_services() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.reload_services_called);
    }

    #[test]
    fn codewalker_execution_gate_does_not_call_set_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert!(!r.set_config_called);
    }

    #[test]
    fn codewalker_execution_gate_null_adapter_still_active() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        assert!(r.null_adapter_active);
        let g = r
            .gates
            .iter()
            .find(|g| g.name == "null_adapter_still_active")
            .unwrap();
        assert!(g.passed);
    }

    #[test]
    fn codewalker_execution_gate_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let r = run(&h, true);
        let out = dir.path().join("codewalker_execution_gate.json");
        fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.is_file());
        let v: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["codewalkerExecutionEligible"], true);
        assert_eq!(v["codewalkerExecutionAllowedNow"], false);
        assert_eq!(v["codewalkerExecutionPerformed"], false);
        assert_eq!(v["writerAllowed"], false);
        assert_eq!(v["modifiesArchive"], false);
        assert_eq!(v["replaceEndpointCalled"], false);
        assert_eq!(v["postRequestsSent"], false);
        assert_eq!(v["httpRequestsSent"], false);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
    }

    #[test]
    fn codewalker_execution_gate_does_not_modify_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = happy(dir.path());
        let before = fs::read(&h.target).unwrap();
        let count_before = fs::read_dir(dir.path()).unwrap().count();
        let _ = run(&h, true);
        let after = fs::read(&h.target).unwrap();
        let count_after = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(before, after);
        assert_eq!(count_before, count_after);
    }
}
