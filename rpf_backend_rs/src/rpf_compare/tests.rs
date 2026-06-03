#[cfg(test)]
mod tests {
    use crate::rpf_compare::compare::compare_rpf_archives;
    use crate::rpf_compare::model::RpfCompareStatus;
    use std::path::Path;

    const FAKE_CLEAN: &str = "../examples/rpf_fixtures/fake_update.rpf";
    const FAKE_MODDED: &str = "../examples/rpf_fixtures/fake_modded_update.rpf";

    fn sha256_of(path: &Path) -> String {
        let data = std::fs::read(path).unwrap();
        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
    }

    fn write_rpf(dir: &Path, name: &str, contents: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn compare_rpf_reads_both_file_metadata() {
        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), Path::new(FAKE_MODDED)).unwrap();
        assert_eq!(report.status, RpfCompareStatus::Compared);
        assert!(report.clean_file_info.exists);
        assert!(report.clean_file_info.is_file);
        assert!(report.clean_file_info.extension_valid);
        assert!(report.modded_file_info.exists);
        assert!(report.modded_file_info.is_file);
        assert!(report.modded_file_info.extension_valid);
        assert_eq!(
            report.clean_size_bytes,
            Some(std::fs::metadata(FAKE_CLEAN).unwrap().len())
        );
        assert_eq!(
            report.modded_size_bytes,
            Some(std::fs::metadata(FAKE_MODDED).unwrap().len())
        );
    }

    #[test]
    fn compare_rpf_computes_both_sha256_hashes() {
        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), Path::new(FAKE_MODDED)).unwrap();
        assert_eq!(report.hash_algorithm, "SHA-256");
        assert_eq!(
            report.clean_sha256.as_deref(),
            Some(sha256_of(Path::new(FAKE_CLEAN)).as_str())
        );
        assert_eq!(
            report.modded_sha256.as_deref(),
            Some(sha256_of(Path::new(FAKE_MODDED)).as_str())
        );
        // file_info mirrors the top-level fields.
        assert_eq!(report.clean_file_info.sha256, report.clean_sha256);
        assert_eq!(report.modded_file_info.sha256, report.modded_sha256);
    }

    #[test]
    fn compare_rpf_detects_hash_difference() {
        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), Path::new(FAKE_MODDED)).unwrap();
        assert!(report.hash_differs);
        assert!(report.archives_differ);
        assert!(report.differences.iter().any(|d| d.kind == "hash"));
    }

    #[test]
    fn compare_rpf_detects_size_difference() {
        let dir = tempfile::TempDir::new().unwrap();
        let a = write_rpf(dir.path(), "clean.rpf", b"short\n");
        let b = write_rpf(
            dir.path(),
            "modded.rpf",
            b"a much longer set of bytes here\n",
        );

        let report = compare_rpf_archives(&a, &b).unwrap();
        assert_eq!(report.status, RpfCompareStatus::Compared);
        assert!(report.size_differs);
        assert!(report.archives_differ);
        assert!(report.differences.iter().any(|d| d.kind == "size"));
    }

    #[test]
    fn compare_rpf_reports_archives_differ_false_for_identical_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let a = write_rpf(dir.path(), "clean.rpf", b"identical bytes\n");
        let b = write_rpf(dir.path(), "modded.rpf", b"identical bytes\n");

        let report = compare_rpf_archives(&a, &b).unwrap();
        assert_eq!(report.status, RpfCompareStatus::Compared);
        assert!(!report.size_differs);
        assert!(!report.hash_differs);
        assert!(!report.archives_differ);
        assert!(report.differences.is_empty());
        assert_eq!(report.clean_sha256, report.modded_sha256);
    }

    #[test]
    fn compare_rpf_blocks_missing_clean_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("nope.rpf");
        let report = compare_rpf_archives(&missing, Path::new(FAKE_MODDED)).unwrap();

        assert_eq!(report.status, RpfCompareStatus::Blocked);
        assert!(!report.clean_file_info.exists);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_clean_target"));
    }

    #[test]
    fn compare_rpf_blocks_missing_modded_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("nope.rpf");
        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), &missing).unwrap();

        assert_eq!(report.status, RpfCompareStatus::Blocked);
        assert!(!report.modded_file_info.exists);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_modded_target"));
    }

    #[test]
    fn compare_rpf_blocks_non_rpf_clean_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        let txt = write_rpf(dir.path(), "not_an_archive.txt", b"hi\n");
        let report = compare_rpf_archives(&txt, Path::new(FAKE_MODDED)).unwrap();

        assert_eq!(report.status, RpfCompareStatus::Blocked);
        assert!(!report.clean_file_info.extension_valid);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "non_rpf_clean_target"));
    }

    #[test]
    fn compare_rpf_blocks_non_rpf_modded_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        let txt = write_rpf(dir.path(), "not_an_archive.txt", b"hi\n");
        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), &txt).unwrap();

        assert_eq!(report.status, RpfCompareStatus::Blocked);
        assert!(!report.modded_file_info.extension_valid);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "non_rpf_modded_target"));
    }

    #[test]
    fn compare_rpf_blocks_directory_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_target = dir.path().join("archive.rpf");
        std::fs::create_dir_all(&dir_target).unwrap();

        let report = compare_rpf_archives(&dir_target, Path::new(FAKE_MODDED)).unwrap();
        assert_eq!(report.status, RpfCompareStatus::Blocked);
        assert!(report.clean_file_info.exists);
        assert!(!report.clean_file_info.is_file);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "clean_target_not_a_file"));
    }

    #[test]
    fn compare_rpf_does_not_modify_either_archive() {
        let clean_before = sha256_of(Path::new(FAKE_CLEAN));
        let modded_before = sha256_of(Path::new(FAKE_MODDED));
        let clean_len_before = std::fs::metadata(FAKE_CLEAN).unwrap().len();
        let modded_len_before = std::fs::metadata(FAKE_MODDED).unwrap().len();

        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), Path::new(FAKE_MODDED)).unwrap();
        assert!(!report.modifies_clean_archive);
        assert!(!report.modifies_modded_archive);

        assert_eq!(clean_before, sha256_of(Path::new(FAKE_CLEAN)));
        assert_eq!(modded_before, sha256_of(Path::new(FAKE_MODDED)));
        assert_eq!(
            clean_len_before,
            std::fs::metadata(FAKE_CLEAN).unwrap().len()
        );
        assert_eq!(
            modded_len_before,
            std::fs::metadata(FAKE_MODDED).unwrap().len()
        );
    }

    #[test]
    fn compare_rpf_reports_native_parser_not_implemented() {
        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), Path::new(FAKE_MODDED)).unwrap();
        assert!(!report.can_compare_internals);
        assert!(!report.native_parser_implemented);
    }

    #[test]
    fn compare_rpf_out_file_written_when_requested() {
        let report = compare_rpf_archives(Path::new(FAKE_CLEAN), Path::new(FAKE_MODDED)).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let out_path = dir.path().join("rpf_compare_report.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();

        assert!(out_path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["archivesDiffer"], true);
        assert_eq!(v["hashDiffers"], true);
        assert_eq!(v["canCompareInternals"], false);
        assert_eq!(v["nativeParserImplemented"], false);
        assert_eq!(v["modifiesCleanArchive"], false);
        assert_eq!(v["modifiesModdedArchive"], false);
        assert!(v["cleanSha256"].is_string());
        assert!(v["moddedSha256"].is_string());
    }
}
