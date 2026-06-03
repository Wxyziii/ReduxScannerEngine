#[cfg(test)]
mod tests {
    use crate::apply::model::ApplyStatus;
    use crate::apply::text_apply::apply_patch_plan_to_stage;
    use crate::staging::stager::stage_patch_plan;
    use std::path::Path;

    const REPLACE_PLAN: &str = "../examples/patch_plans/valid_text_replace_patch.json";
    const APPEND_PLAN: &str = "../examples/patch_plans/valid_text_append_patch.json";
    const PREPEND_PLAN: &str = "../examples/patch_plans/valid_text_prepend_patch.json";
    const UNSUPPORTED_PLAN: &str = "../examples/patch_plans/unsupported_operation_patch.json";
    const FULL_WS: &str = "../examples/workspaces/update_rpf_fixture";

    // Helper: stage a plan into a fresh TempDir using the full workspace.
    fn stage_into_dir(plan: &str, dir: &tempfile::TempDir) {
        let report = stage_patch_plan(Path::new(plan), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(
            report.safe_to_stage,
            "staging failed for '{}': {:?}",
            plan, report.blocked
        );
    }

    // ── T0.4.4: apply-stage tests ──────────────────────────────────────────────

    #[test]
    fn apply_stage_replace_exact_string_modifies_staged_file() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_into_dir(REPLACE_PLAN, &dir);

        let report = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();
        assert!(
            report.safe_applied,
            "expected safe_applied=true; blocked={:?}",
            report.blocked
        );
        assert_eq!(report.status, ApplyStatus::AllApplied);

        let staged_path = dir.path().join("common/data/visualsettings.dat");
        let content = std::fs::read_to_string(&staged_path).unwrap();
        assert!(
            content.contains("Gamma 2.400000"),
            "expected 'Gamma 2.400000' in staged file after replace"
        );
        assert!(
            !content.contains("Gamma 2.200000"),
            "original 'Gamma 2.200000' should be replaced"
        );
    }

    #[test]
    fn apply_stage_append_modifies_staged_file() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_into_dir(APPEND_PLAN, &dir);

        let report = apply_patch_plan_to_stage(Path::new(APPEND_PLAN), dir.path()).unwrap();
        assert!(report.safe_applied, "blocked={:?}", report.blocked);
        assert_eq!(report.status, ApplyStatus::AllApplied);

        let staged_path = dir.path().join("common/data/visualsettings.dat");
        let content = std::fs::read_to_string(&staged_path).unwrap();
        assert!(
            content.contains("ExposureMin 0.001000"),
            "expected 'ExposureMin 0.001000' appended to staged file"
        );
    }

    #[test]
    fn apply_stage_prepend_modifies_staged_file() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_into_dir(PREPEND_PLAN, &dir);

        let report = apply_patch_plan_to_stage(Path::new(PREPEND_PLAN), dir.path()).unwrap();
        assert!(report.safe_applied, "blocked={:?}", report.blocked);
        assert_eq!(report.status, ApplyStatus::AllApplied);

        let staged_path = dir.path().join("common/data/visualsettings.dat");
        let content = std::fs::read_to_string(&staged_path).unwrap();
        assert!(
            content.starts_with("# Redux patch header"),
            "expected '# Redux patch header' at start of staged file"
        );
    }

    #[test]
    fn apply_stage_does_not_modify_source_workspace() {
        let source_file = Path::new(FULL_WS).join("common/data/visualsettings.dat");
        let hash_before = {
            let data = std::fs::read(&source_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };

        let dir = tempfile::TempDir::new().unwrap();
        stage_into_dir(REPLACE_PLAN, &dir);
        apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();

        let hash_after = {
            let data = std::fs::read(&source_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };
        assert_eq!(
            hash_before, hash_after,
            "source workspace file was modified during apply-stage"
        );
    }

    #[test]
    fn apply_stage_missing_staged_target_blocks() {
        // Empty TempDir — no staged files present.
        let dir = tempfile::TempDir::new().unwrap();

        let report = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();
        assert!(
            !report.safe_applied,
            "expected safe_applied=false for missing staged target"
        );
        assert_eq!(report.status, ApplyStatus::Blocked);
        let has_missing = report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_staged_file");
        assert!(has_missing, "expected 'missing_staged_file' block type");
    }

    #[test]
    fn apply_stage_unsupported_operation_blocks() {
        let dir = tempfile::TempDir::new().unwrap();
        // The unsupported plan (dat_named_key_candidate) passes staging but must be
        // blocked by the apply engine with "unsupported_op_type".
        stage_into_dir(UNSUPPORTED_PLAN, &dir);

        let report = apply_patch_plan_to_stage(Path::new(UNSUPPORTED_PLAN), dir.path()).unwrap();
        assert!(
            !report.safe_applied,
            "expected safe_applied=false for unsupported operation"
        );
        assert_eq!(report.status, ApplyStatus::Blocked);
        let has_unsupported = report
            .blocked
            .iter()
            .any(|b| b.block_type == "unsupported_op_type");
        assert!(
            has_unsupported,
            "expected 'unsupported_op_type' block; got: {:?}",
            report.blocked
        );
    }

    #[test]
    fn apply_stage_binary_target_blocks() {
        let dir = tempfile::TempDir::new().unwrap();

        // Manually place a binary file at the expected path.
        let target_dir = dir.path().join("common/data");
        std::fs::create_dir_all(&target_dir).unwrap();
        let target_path = target_dir.join("visualsettings.dat");
        // Write binary content with null bytes.
        std::fs::write(&target_path, b"binary\x00data\x00here").unwrap();

        let report = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();
        assert!(
            !report.safe_applied,
            "expected safe_applied=false for binary staged file"
        );
        assert_eq!(report.status, ApplyStatus::Blocked);
        let has_binary = report.blocked.iter().any(|b| b.block_type == "binary_file");
        assert!(
            has_binary,
            "expected 'binary_file' block; got: {:?}",
            report.blocked
        );
    }

    #[test]
    fn apply_stage_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_into_dir(REPLACE_PLAN, &dir);

        let report = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();
        let out_path = dir.path().join("apply_report.json");
        let json = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write(&out_path, &json).unwrap();

        assert!(out_path.exists(), "output file should exist");
        let content = std::fs::read_to_string(&out_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["safeApplied"], true);
        assert_eq!(parsed["status"], "all_applied");
        assert!(parsed["summary"].is_object());
    }

    #[test]
    fn apply_stage_report_safe_false_when_any_operation_blocked() {
        // Missing staged file → report.safe_applied must be false.
        let dir = tempfile::TempDir::new().unwrap();
        let report = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();
        assert!(!report.safe_applied);
        assert_eq!(report.status, ApplyStatus::Blocked);
        assert_eq!(report.summary.applied_count, 0);
        assert!(report.summary.blocked_count > 0);
    }

    #[test]
    fn apply_stage_report_safe_true_when_all_operations_apply() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_into_dir(REPLACE_PLAN, &dir);

        let report = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();
        assert!(report.safe_applied);
        assert_eq!(report.status, ApplyStatus::AllApplied);
        assert!(report.blocked.is_empty());
        assert_eq!(report.summary.blocked_count, 0);
        assert!(report.summary.applied_count > 0);
    }
}
