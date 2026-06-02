#[cfg(test)]
mod rollback_restore_tests {
    use crate::codewalker_api::model::CodeWalkerRollbackRestoreStatus;
    use crate::codewalker_api::rollback_restore::{
        execute_codewalker_rollback_restore, CONFIRMATION_PHRASE,
    };
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};

    const BACKUP_CONTENT: &[u8] = b"FAKE-RPF verified backup content\n";
    const TARGET_MODIFIED: &[u8] = b"FAKE-RPF modified/corrupt target content - longer\n";

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        format!("{:x}", h.finalize())
    }

    fn write_file(p: &Path, bytes: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn write_json(dir: &Path, name: &str, v: &Value) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
        p
    }

    fn post_write_report(dir: &Path, ready: bool, available: bool, confirmed: bool) -> PathBuf {
        write_json(
            dir,
            "codewalker_post_write_verify.json",
            &json!({
                "rollbackAvailable": available,
                "rollbackExecuted": false,
                "copiedTestArchiveConfirmed": confirmed,
                "rollbackPlan": {
                    "rollbackPlanStatus": if ready { "ready" } else { "unavailable" }
                }
            }),
        )
    }

    /// Backup report + a real backup file holding BACKUP_CONTENT.
    fn backup_report(
        dir: &Path,
        target: &Path,
        valid: bool,
        real_hash: bool,
    ) -> (PathBuf, PathBuf) {
        let backup_file = dir.join("backups/fake_update.rpf.bak");
        write_file(&backup_file, BACKUP_CONTENT);
        let reported_hash = if real_hash {
            sha256_hex(BACKUP_CONTENT)
        } else {
            "deadbeef".repeat(8)
        };
        let p = write_json(
            dir,
            "rpf_backup_report.json",
            &json!({
                "status": "backed_up",
                "targetArchivePath": target.display().to_string(),
                "backupFilePath": backup_file.display().to_string(),
                "originalHash": sha256_hex(BACKUP_CONTENT),
                "backupHash": reported_hash,
                "hashVerified": valid,
                "safeForFutureWrite": valid
            }),
        );
        (p, backup_file)
    }

    struct Setup {
        target: PathBuf,
        pw: PathBuf,
        backup: PathBuf,
        backup_file: PathBuf,
    }

    /// Copied-test target (different content from backup) + valid reports.
    fn setup(dir: &Path) -> Setup {
        let target = dir.join("test_copies/fake_update_rollback_target.rpf");
        write_file(&target, TARGET_MODIFIED);
        let pw = post_write_report(dir, true, true, true);
        let (backup, backup_file) = backup_report(dir, &target, true, true);
        Setup {
            target,
            pw,
            backup,
            backup_file,
        }
    }

    fn run(
        s: &Setup,
        execute: bool,
        confirm: Option<&str>,
    ) -> crate::codewalker_api::model::CodeWalkerRollbackRestoreReport {
        execute_codewalker_rollback_restore(&s.target, &s.pw, &s.backup, execute, confirm).unwrap()
    }

    // ── Blocked cases (no copy) ─────────────────────────────────────────────

    #[test]
    fn codewalker_rollback_restore_blocks_without_execute_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let before = fs::read(&s.target).unwrap();
        let r = run(&s, false, Some(CONFIRMATION_PHRASE));
        assert_eq!(r.status, CodeWalkerRollbackRestoreStatus::Blocked);
        assert!(!r.rollback_executed);
        assert!(!r.modifies_archive);
        assert_eq!(fs::read(&s.target).unwrap(), before);
    }

    #[test]
    fn codewalker_rollback_restore_blocks_missing_confirmation_phrase() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, None);
        assert!(!r.confirmation_phrase_provided);
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), TARGET_MODIFIED);
    }

    #[test]
    fn codewalker_rollback_restore_blocks_wrong_confirmation_phrase() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some("nope"));
        assert!(r.confirmation_phrase_provided);
        assert!(!r.confirmation_phrase_matched);
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), TARGET_MODIFIED);
    }

    #[test]
    fn codewalker_rollback_restore_blocks_missing_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = setup(dir.path());
        s.target = dir.path().join("test_copies/missing.rpf");
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.target_rpf_exists);
        assert!(!r.rollback_executed);
    }

    #[test]
    fn codewalker_rollback_restore_blocks_non_rpf_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = setup(dir.path());
        let bin = dir.path().join("test_copies/target.bin");
        write_file(&bin, TARGET_MODIFIED);
        s.target = bin;
        // Backup target field still points at the old path -> also mismatched, but
        // the extension gate alone must block.
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.target_rpf_extension_valid);
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), TARGET_MODIFIED);
    }

    #[test]
    fn codewalker_rollback_restore_blocks_original_game_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let target = dir
            .path()
            .join("steamapps/common/Grand Theft Auto V/update/update.rpf");
        write_file(&target, TARGET_MODIFIED);
        let pw = post_write_report(dir.path(), true, true, true);
        let (backup, _bf) = backup_report(dir.path(), &target, true, true);
        let r = execute_codewalker_rollback_restore(
            &target,
            &pw,
            &backup,
            true,
            Some(CONFIRMATION_PHRASE),
        )
        .unwrap();
        assert!(!r.target_not_original_game_archive);
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&target).unwrap(), TARGET_MODIFIED);
    }

    // ── Report reading / gating ─────────────────────────────────────────────

    #[test]
    fn codewalker_rollback_restore_reads_post_write_verify_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(r.rollback_plan_ready);
        assert!(r.rollback_available);
        assert!(r.copied_test_archive_confirmed);
    }

    #[test]
    fn codewalker_rollback_restore_requires_rollback_plan_ready() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = setup(dir.path());
        s.pw = post_write_report(dir.path(), false, true, true); // plan not ready
        let before = fs::read(&s.target).unwrap();
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.rollback_plan_ready);
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), before);
    }

    #[test]
    fn codewalker_rollback_restore_reads_backup_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(r.backup_hash_verified);
        assert!(r.backup_safe_for_future_write);
        assert!(r.backup_file_exists);
    }

    #[test]
    fn codewalker_rollback_restore_requires_backup_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = setup(dir.path());
        // Backup report referencing a non-existent backup file.
        s.backup = write_json(
            dir.path(),
            "rpf_backup_report.json",
            &json!({
                "targetArchivePath": s.target.display().to_string(),
                "backupFilePath": dir.path().join("backups/missing.bak").display().to_string(),
                "backupHash": sha256_hex(BACKUP_CONTENT),
                "hashVerified": true,
                "safeForFutureWrite": true
            }),
        );
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.backup_file_exists);
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), TARGET_MODIFIED);
    }

    #[test]
    fn codewalker_rollback_restore_requires_backup_hash_verified() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = setup(dir.path());
        let (backup, _bf) = backup_report(dir.path(), &s.target, false, true); // not verified
        s.backup = backup;
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.backup_hash_verified);
        assert!(!r.rollback_executed);
    }

    #[test]
    fn codewalker_rollback_restore_requires_backup_hash_match() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = setup(dir.path());
        let (backup, _bf) = backup_report(dir.path(), &s.target, true, false); // reported hash wrong
        s.backup = backup;
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.backup_hash_matches_report);
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), TARGET_MODIFIED);
    }

    #[test]
    fn codewalker_rollback_restore_requires_backup_target_match() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = setup(dir.path());
        // Backup report targets a different path.
        s.backup = write_json(
            dir.path(),
            "rpf_backup_report.json",
            &json!({
                "targetArchivePath": "some/other/archive.rpf",
                "backupFilePath": s.backup_file.display().to_string(),
                "backupHash": sha256_hex(BACKUP_CONTENT),
                "hashVerified": true,
                "safeForFutureWrite": true
            }),
        );
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert_eq!(r.backup_target_matches_target, Some(false));
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), TARGET_MODIFIED);
    }

    // ── Allowed restore ─────────────────────────────────────────────────────

    #[test]
    fn codewalker_rollback_restore_executes_copy_when_all_gates_pass() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert_eq!(r.status, CodeWalkerRollbackRestoreStatus::Restored);
        assert!(r.rollback_executed);
        assert!(r.rollback_execution_allowed);
        // Target now equals the backup content.
        assert_eq!(fs::read(&s.target).unwrap(), BACKUP_CONTENT);
    }

    #[test]
    fn codewalker_rollback_restore_restored_target_matches_backup() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert_eq!(r.restored_target_matches_backup, Some(true));
        assert_eq!(r.summary.restored_target_matches_backup, Some(true));
    }

    #[test]
    fn codewalker_rollback_restore_records_before_and_after_hash() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert_eq!(
            r.target_sha256_before.as_deref(),
            Some(sha256_hex(TARGET_MODIFIED).as_str())
        );
        assert_eq!(
            r.target_sha256_after.as_deref(),
            Some(sha256_hex(BACKUP_CONTENT).as_str())
        );
    }

    #[test]
    fn codewalker_rollback_restore_modifies_archive_true_only_when_restored() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let allowed = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(allowed.modifies_archive);

        let dir2 = tempfile::TempDir::new().unwrap();
        let s2 = setup(dir2.path());
        let blocked = run(&s2, false, Some(CONFIRMATION_PHRASE));
        assert!(!blocked.modifies_archive);
    }

    // ── Safety: no network / endpoints ──────────────────────────────────────

    #[test]
    fn codewalker_rollback_restore_does_not_send_http_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.http_requests_sent);
    }

    #[test]
    fn codewalker_rollback_restore_does_not_use_post() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.post_requests_sent);
    }

    #[test]
    fn codewalker_rollback_restore_does_not_call_replace_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.replace_endpoint_called);
    }

    #[test]
    fn codewalker_rollback_restore_does_not_call_import_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.import_endpoint_called);
    }

    #[test]
    fn codewalker_rollback_restore_does_not_call_reload_services() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.reload_services_called);
    }

    #[test]
    fn codewalker_rollback_restore_does_not_call_set_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.set_config_called);
    }

    #[test]
    fn codewalker_rollback_restore_does_not_execute_external_tool() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        assert!(!r.external_tool_executed);
        assert!(!r.native_parser_used);
        assert!(!r.native_writer_used);
        assert!(!r.writer_allowed);
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
    }

    #[test]
    fn codewalker_rollback_restore_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let r = run(&s, true, Some(CONFIRMATION_PHRASE));
        let out = dir.path().join("codewalker_rollback_restore.json");
        fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["rollbackExecuted"], true);
        assert_eq!(v["restoredTargetMatchesBackup"], true);
        assert_eq!(v["modifiesArchive"], true);
        assert_eq!(v["restoreMethod"], "copy_backup_over_target");
        assert_eq!(v["httpRequestsSent"], false);
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
    }

    #[test]
    fn codewalker_rollback_restore_does_not_modify_when_blocked() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = setup(dir.path());
        let before = fs::read(&s.target).unwrap();
        // Blocked: no execute flag.
        let r = run(&s, false, Some(CONFIRMATION_PHRASE));
        assert!(!r.rollback_executed);
        assert_eq!(fs::read(&s.target).unwrap(), before);
        assert_eq!(fs::read(&s.target).unwrap(), TARGET_MODIFIED);
    }
}
