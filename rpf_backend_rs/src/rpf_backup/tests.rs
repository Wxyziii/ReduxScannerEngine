#[cfg(test)]
mod tests {
    use crate::rpf_backup::backup::backup_rpf_archive;
    use crate::rpf_backup::model::RpfBackupStatus;
    use std::path::Path;

    const FAKE_RPF: &str = "../examples/rpf_fixtures/fake_update.rpf";

    fn sha256_of(path: &Path) -> String {
        let data = std::fs::read(path).unwrap();
        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
    }

    #[test]
    fn backup_rpf_creates_backup_file() {
        let backup = tempfile::TempDir::new().unwrap();
        let report = backup_rpf_archive(Path::new(FAKE_RPF), backup.path()).unwrap();

        assert_eq!(report.status, RpfBackupStatus::BackedUp);
        let bpath = report.backup_file_path.expect("backup path");
        assert!(Path::new(&bpath).is_file(), "backup file should exist");
    }

    #[test]
    fn backup_rpf_verifies_hash_match() {
        let backup = tempfile::TempDir::new().unwrap();
        let report = backup_rpf_archive(Path::new(FAKE_RPF), backup.path()).unwrap();

        assert!(report.hash_verified);
        assert_eq!(report.hash_algorithm, "SHA-256");
        assert_eq!(report.original_hash, report.backup_hash);
        // The recorded original hash must equal the actual fixture hash.
        assert_eq!(
            report.original_hash.as_deref(),
            Some(sha256_of(Path::new(FAKE_RPF)).as_str())
        );
    }

    #[test]
    fn backup_rpf_blocks_missing_target() {
        let backup = tempfile::TempDir::new().unwrap();
        let missing = backup.path().join("does_not_exist.rpf");
        let report = backup_rpf_archive(&missing, backup.path()).unwrap();

        assert_eq!(report.status, RpfBackupStatus::Blocked);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_target"));
        assert!(!report.safe_for_future_write);
    }

    #[test]
    fn backup_rpf_blocks_non_rpf_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        let txt = dir.path().join("not_an_archive.txt");
        std::fs::write(&txt, b"hello\n").unwrap();
        let backup = tempfile::TempDir::new().unwrap();

        let report = backup_rpf_archive(&txt, backup.path()).unwrap();
        assert_eq!(report.status, RpfBackupStatus::Blocked);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "non_rpf_target"));
        assert!(!report.safe_for_future_write);
    }

    #[test]
    fn backup_rpf_blocks_directory_target() {
        let dir = tempfile::TempDir::new().unwrap();
        // A directory whose name ends with .rpf.
        let dir_target = dir.path().join("archive.rpf");
        std::fs::create_dir_all(&dir_target).unwrap();
        let backup = tempfile::TempDir::new().unwrap();

        let report = backup_rpf_archive(&dir_target, backup.path()).unwrap();
        assert_eq!(report.status, RpfBackupStatus::Blocked);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "target_not_a_file"));
        assert!(!report.safe_for_future_write);
    }

    #[test]
    fn backup_rpf_does_not_modify_original() {
        let backup = tempfile::TempDir::new().unwrap();
        let before = sha256_of(Path::new(FAKE_RPF));
        let before_meta = std::fs::metadata(FAKE_RPF).unwrap().len();

        backup_rpf_archive(Path::new(FAKE_RPF), backup.path()).unwrap();

        let after = sha256_of(Path::new(FAKE_RPF));
        let after_meta = std::fs::metadata(FAKE_RPF).unwrap().len();
        assert_eq!(before, after, "original archive must not change");
        assert_eq!(before_meta, after_meta, "original size must not change");
    }

    #[test]
    fn backup_rpf_out_file_written_when_requested() {
        let backup = tempfile::TempDir::new().unwrap();
        let report = backup_rpf_archive(Path::new(FAKE_RPF), backup.path()).unwrap();

        let out_path = backup.path().join("rpf_backup_report.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        assert!(out_path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["safeForFutureWrite"], true);
        assert_eq!(v["hashVerified"], true);
        assert_eq!(v["hashAlgorithm"], "SHA-256");
    }

    #[test]
    fn backup_rpf_safe_for_future_write_true_on_success() {
        let backup = tempfile::TempDir::new().unwrap();
        let report = backup_rpf_archive(Path::new(FAKE_RPF), backup.path()).unwrap();
        assert!(report.safe_for_future_write);
        assert_eq!(report.summary.blocked_count, 0);
        assert!(report.summary.backup_created);
        // Even on success, no real writer is enabled.
        assert!(!report.real_writer_implemented);
        assert!(!report.modifies_target_archive);
    }

    #[test]
    fn backup_rpf_safe_for_future_write_false_when_blocked() {
        let backup = tempfile::TempDir::new().unwrap();
        let missing = backup.path().join("nope.rpf");
        let report = backup_rpf_archive(&missing, backup.path()).unwrap();
        assert!(!report.safe_for_future_write);
        assert!(report.summary.blocked_count >= 1);
        assert!(report.backup_file_path.is_none());
    }

    #[test]
    fn backup_rpf_creates_backup_dir_if_missing() {
        let parent = tempfile::TempDir::new().unwrap();
        let nested = parent.path().join("new/backup/dir");
        assert!(!nested.exists());

        let report = backup_rpf_archive(Path::new(FAKE_RPF), &nested).unwrap();
        assert_eq!(report.status, RpfBackupStatus::BackedUp);
        assert!(nested.is_dir(), "backup dir should be created");
    }
}
