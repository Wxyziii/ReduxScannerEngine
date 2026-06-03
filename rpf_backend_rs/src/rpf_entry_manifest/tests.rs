#[cfg(test)]
mod tests {
    use crate::rpf_entry_manifest::manifest::build_rpf_entry_manifest;
    use crate::rpf_entry_manifest::model::RpfEntryManifestStatus;
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    const FAKE_RPF: &str = "../examples/rpf_fixtures/fake_update.rpf";

    /// Write a bundle manifest with the given declared file relative paths.
    fn write_manifest(bundle: &Path, declared: &[&str]) {
        let files: Vec<Value> = declared
            .iter()
            .map(|p| {
                serde_json::json!({
                    "relativePath": p,
                    "exportedPath": format!("files/{}", p),
                    "sizeBytes": 4,
                    "extension": "dat",
                    "sha256": null
                })
            })
            .collect();
        let manifest = serde_json::json!({
            "bundleFormat": "redux_patch_bundle",
            "modifiesRpf": false,
            "modifiesSourceWorkspace": false,
            "exportedFromStageOnly": true,
            "files": files
        });
        std::fs::write(
            bundle.join("bundle_manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    /// Write an actual exported file under files/<rel>.
    fn write_file(bundle: &Path, rel: &str, content: &[u8]) {
        let dst = bundle
            .join("files")
            .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::write(dst, content).unwrap();
    }

    /// A valid bundle with one nested file.
    fn valid_bundle(dir: &Path) -> PathBuf {
        let bundle = dir.join("bundle");
        std::fs::create_dir_all(bundle.join("files")).unwrap();
        write_file(&bundle, "common/data/visualsettings.dat", b"DATA");
        write_manifest(&bundle, &["common/data/visualsettings.dat"]);
        bundle
    }

    #[test]
    fn rpf_entry_manifest_reads_bundle_manifest() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = valid_bundle(dir.path());
        let report = build_rpf_entry_manifest(&bundle, Some(Path::new(FAKE_RPF))).unwrap();
        assert_eq!(report.status, RpfEntryManifestStatus::Built);
        assert_eq!(report.target_rpf.as_deref(), Some(FAKE_RPF));
    }

    #[test]
    fn rpf_entry_manifest_lists_exported_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = valid_bundle(dir.path());
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        assert_eq!(report.manifest.entries.len(), 1);
        let e = &report.manifest.entries[0];
        assert_eq!(e.replacement_source, "bundle/files");
        assert_eq!(e.operation_kind, "replace_file_planned");
        assert!(e.would_replace_existing_entry);
        assert_eq!(
            e.bundle_file_relative_path,
            "files/common/data/visualsettings.dat"
        );
    }

    #[test]
    fn rpf_entry_manifest_computes_sha256() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = valid_bundle(dir.path());
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        let e = &report.manifest.entries[0];
        assert_eq!(e.hash_algorithm, "SHA-256");
        let expected = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(b"DATA"));
        assert_eq!(e.sha256.as_deref(), Some(expected.as_str()));
        assert_eq!(e.size_bytes, 4);
    }

    #[test]
    fn rpf_entry_manifest_preserves_relative_paths() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = valid_bundle(dir.path());
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        assert_eq!(
            report.manifest.entries[0].archive_relative_path,
            "common/data/visualsettings.dat"
        );
        assert!(report.manifest.entries[0].safe_path);
    }

    #[test]
    fn rpf_entry_manifest_blocks_missing_bundle_manifest() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = dir.path().join("bundle");
        std::fs::create_dir_all(bundle.join("files")).unwrap();
        write_file(&bundle, "a.dat", b"x");
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        assert_eq!(report.status, RpfEntryManifestStatus::Blocked);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_bundle_manifest"));
    }

    #[test]
    fn rpf_entry_manifest_blocks_missing_files_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = dir.path().join("bundle");
        std::fs::create_dir_all(&bundle).unwrap();
        write_manifest(&bundle, &[]);
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        assert_eq!(report.status, RpfEntryManifestStatus::Blocked);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_files_dir"));
    }

    #[test]
    fn rpf_entry_manifest_blocks_empty_files_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = dir.path().join("bundle");
        std::fs::create_dir_all(bundle.join("files")).unwrap();
        write_manifest(&bundle, &[]);
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        assert_eq!(report.status, RpfEntryManifestStatus::Blocked);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "empty_files_dir"));
    }

    #[test]
    fn rpf_entry_manifest_blocks_parent_traversal_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = dir.path().join("bundle");
        std::fs::create_dir_all(bundle.join("files")).unwrap();
        // A real, safe file keeps files/ non-empty…
        write_file(&bundle, "ok.dat", b"x");
        // …but the manifest declares an unsafe traversal path.
        write_manifest(&bundle, &["common/../../escape.dat"]);
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        assert_eq!(report.status, RpfEntryManifestStatus::Blocked);
        assert!(report.blocked.iter().any(|b| b.block_type == "unsafe_path"));
        assert!(report.summary.unsafe_path_count >= 1);
    }

    #[test]
    fn rpf_entry_manifest_detects_duplicate_targets() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = dir.path().join("bundle");
        std::fs::create_dir_all(bundle.join("files")).unwrap();
        write_file(&bundle, "dup.dat", b"x");
        write_manifest(&bundle, &["dup.dat", "dup.dat"]);
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();
        assert_eq!(report.status, RpfEntryManifestStatus::Blocked);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "duplicate_target"));
        assert!(report.summary.duplicate_count >= 1);
    }

    #[test]
    fn rpf_entry_manifest_ready_for_write_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = valid_bundle(dir.path());
        let report = build_rpf_entry_manifest(&bundle, Some(Path::new(FAKE_RPF))).unwrap();
        assert!(!report.ready_for_write);
        assert!(!report.manifest.ready_for_write);
        assert!(!report.modifies_rpf);
        assert!(!report.native_parser_used);
        assert!(!report.native_writer_used);
        assert!(!report.external_tool_used);
    }

    #[test]
    fn rpf_entry_manifest_does_not_modify_bundle_or_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = valid_bundle(dir.path());

        let target_before = std::fs::read(FAKE_RPF).unwrap();
        let file_path = bundle.join("files/common/data/visualsettings.dat");
        let file_before = std::fs::read(&file_path).unwrap();
        let manifest_before = std::fs::read(bundle.join("bundle_manifest.json")).unwrap();

        build_rpf_entry_manifest(&bundle, Some(Path::new(FAKE_RPF))).unwrap();

        assert_eq!(target_before, std::fs::read(FAKE_RPF).unwrap());
        assert_eq!(file_before, std::fs::read(&file_path).unwrap());
        assert_eq!(
            manifest_before,
            std::fs::read(bundle.join("bundle_manifest.json")).unwrap()
        );
    }

    #[test]
    fn rpf_entry_manifest_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let bundle = valid_bundle(dir.path());
        let report = build_rpf_entry_manifest(&bundle, None).unwrap();

        let out_path = dir.path().join("rpf_entry_manifest.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        assert!(out_path.is_file());

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["readyForWrite"], false);
        assert_eq!(v["modifiesRpf"], false);
        assert_eq!(v["nativeParserUsed"], false);
        assert_eq!(v["nativeWriterUsed"], false);
        assert_eq!(v["externalToolUsed"], false);
        assert_eq!(v["manifest"]["manifestVersion"], "1");
    }
}
