#[cfg(test)]
mod tests {
    use crate::staging::stager::stage_patch_plan;
    use std::path::Path;

    const VALID_PLAN: &str = "../examples/patch_plans/valid_first_patch.json";
    const FULL_WS: &str = "../examples/workspaces/update_rpf_fixture";
    const PARTIAL_WS: &str = "../examples/workspaces/partial_rpf_fixture";
    const BLOCKED_PLAN: &str =
        "../examples/editor_fixtures/invalid_editor_weather_xml_operation.json";

    // ── T0.4.3: staging tests ────────────────────────────────────────────────

    #[test]
    fn stage_valid_patch_plan_copies_target_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(VALID_PLAN), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(
            report.safe_to_stage,
            "expected safe_to_stage=true; blocked={:?}",
            report.blocked
        );
        assert_eq!(report.files.len(), 3, "expected 3 staged files");
        for f in &report.files {
            assert!(
                std::path::Path::new(&f.staged_abs).exists(),
                "staged file missing: {}",
                f.staged_abs
            );
        }
    }

    #[test]
    fn stage_writes_stage_manifest() {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(VALID_PLAN), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(report.manifest_path.is_some());
        let manifest_path = report.manifest_path.as_ref().unwrap();
        assert!(
            std::path::Path::new(manifest_path).exists(),
            "manifest not found: {}",
            manifest_path
        );
        let content = std::fs::read_to_string(manifest_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["safeToStage"], true);
        assert!(parsed["files"].is_array());
        assert_eq!(parsed["files"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn stage_missing_target_blocks_without_copying() {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(VALID_PLAN), Path::new(PARTIAL_WS), dir.path()).unwrap();
        assert!(!report.safe_to_stage, "expected safe_to_stage=false");
        assert!(report.files.is_empty(), "expected no files copied");
        assert!(
            !report.blocked.is_empty(),
            "expected blocked items to be non-empty"
        );
        // stage_manifest.json must NOT have been written
        assert!(!dir.path().join("stage_manifest.json").exists());
    }

    #[test]
    fn stage_blocked_scope_blocks_without_copying() {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(BLOCKED_PLAN), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(!report.safe_to_stage, "expected safe_to_stage=false");
        assert!(report.files.is_empty(), "expected no files copied");
        let has_scope_block = report
            .blocked
            .iter()
            .any(|b| b.block_type == "blocked_deferred" || b.block_type == "not_in_scope");
        assert!(has_scope_block, "expected a scope-related block type");
    }

    #[test]
    fn stage_preserves_relative_paths() {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(VALID_PLAN), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(report.safe_to_stage);
        for f in &report.files {
            // staged_path should match source_path exactly
            assert_eq!(
                f.source_path, f.staged_path,
                "source and staged relative paths diverged"
            );
            // the file should exist at stage_dir / staged_path
            let expected = dir
                .path()
                .join(f.staged_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            assert!(
                expected.exists(),
                "expected staged file at: {}",
                expected.display()
            );
        }
    }

    #[test]
    fn stage_does_not_modify_workspace_file() {
        let source_file = Path::new(FULL_WS).join("common/data/visualsettings.dat");
        let hash_before = {
            let data = std::fs::read(&source_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };

        let dir = tempfile::TempDir::new().unwrap();
        stage_patch_plan(Path::new(VALID_PLAN), Path::new(FULL_WS), dir.path()).unwrap();

        let hash_after = {
            let data = std::fs::read(&source_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };
        assert_eq!(
            hash_before, hash_after,
            "workspace source file was modified during staging"
        );
    }

    #[test]
    fn stage_cli_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(VALID_PLAN), Path::new(FULL_WS), dir.path()).unwrap();
        let out_path = dir.path().join("stage_report.json");
        let json = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write(&out_path, &json).unwrap();
        assert!(out_path.exists());
        let content = std::fs::read_to_string(&out_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["safeToStage"], true);
        assert!(parsed["files"].as_array().unwrap().len() == 3);
        assert!(parsed.get("summary").is_some());
    }

    #[test]
    fn stage_report_safe_false_when_dry_run_blocked() {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(BLOCKED_PLAN), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(!report.safe_to_stage);
        assert_eq!(report.status, crate::staging::model::StageStatus::Blocked);
        assert_eq!(report.summary.staged_count, 0);
    }
}
