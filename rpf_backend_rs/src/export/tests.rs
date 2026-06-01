#[cfg(test)]
mod tests {
    use crate::apply::text_apply::apply_patch_plan_to_stage;
    use crate::export::bundle::export_patch_bundle;
    use crate::staging::stager::stage_patch_plan;
    use std::path::Path;

    const REPLACE_PLAN: &str = "../examples/patch_plans/valid_text_replace_patch.json";
    const FULL_WS: &str = "../examples/workspaces/update_rpf_fixture";
    const TARGET_REL: &str = "common/data/visualsettings.dat";

    /// Stage + apply the replace plan into a fresh stage dir. Returns the dir.
    fn staged_and_applied() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        let report =
            stage_patch_plan(Path::new(REPLACE_PLAN), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(report.safe_to_stage, "staging failed: {:?}", report.blocked);
        let apply = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), dir.path()).unwrap();
        assert!(apply.safe_applied, "apply failed: {:?}", apply.blocked);
        dir
    }

    fn sha256_of(path: &Path) -> String {
        let data = std::fs::read(path).unwrap();
        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
    }

    #[test]
    fn export_bundle_copies_patched_files() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        assert!(report.safe_exported, "blocked: {:?}", report.blocked);
        let copied = bundle.path().join("files").join(TARGET_REL);
        assert!(copied.is_file(), "patched file not copied into bundle");
        // The copied file should match the staged (patched) content, not the workspace.
        let staged_file = stage.path().join(TARGET_REL);
        assert_eq!(sha256_of(&copied), sha256_of(&staged_file));
    }

    #[test]
    fn export_bundle_preserves_relative_paths() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        let f = report
            .files
            .iter()
            .find(|f| f.relative_path == TARGET_REL)
            .expect("target should be in bundle files");
        assert!(!f.relative_path.starts_with('/'));
        assert!(!f.relative_path.contains('\\'));
        assert!(bundle.path().join("files").join(TARGET_REL).is_file());
    }

    #[test]
    fn export_bundle_writes_bundle_manifest() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        let manifest_path = bundle.path().join("bundle_manifest.json");
        assert!(manifest_path.is_file(), "bundle_manifest.json missing");
        assert_eq!(
            report.manifest_path.as_deref(),
            Some(manifest_path.to_string_lossy().as_ref())
        );

        let content = std::fs::read_to_string(&manifest_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["bundleFormat"], "redux_patch_bundle");
        assert_eq!(v["modifiesRpf"], false);
        assert_eq!(v["modifiesSourceWorkspace"], false);
        assert_eq!(v["exportedFromStageOnly"], true);
        assert!(v["files"].is_array());
    }

    #[test]
    fn export_bundle_copies_patch_plan() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();
        export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        let plan_in_bundle = bundle.path().join("patch_plan.json");
        assert!(plan_in_bundle.is_file(), "patch_plan.json missing");
        assert_eq!(
            sha256_of(&plan_in_bundle),
            sha256_of(Path::new(REPLACE_PLAN))
        );
    }

    #[test]
    fn export_bundle_generates_diff_report() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();
        export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        let diff_path = bundle.path().join("diff_report.json");
        assert!(diff_path.is_file(), "diff_report.json missing");
        let content = std::fs::read_to_string(&diff_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(v["files"].is_array());
        assert!(v["summary"].is_object());
    }

    #[test]
    fn export_bundle_blocks_missing_stage_dir() {
        let bundle = tempfile::TempDir::new().unwrap();
        let missing = bundle.path().join("does_not_exist_stage");
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            &missing,
            &bundle.path().join("out_bundle"),
        )
        .unwrap();

        assert!(!report.safe_exported);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_stage_dir"));
    }

    #[test]
    fn export_bundle_blocks_empty_stage_dir() {
        let stage = tempfile::TempDir::new().unwrap();
        let bundle = tempfile::TempDir::new().unwrap();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        assert!(!report.safe_exported);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "empty_stage_dir"));
    }

    #[test]
    fn export_bundle_blocks_bundle_dir_equal_stage_dir() {
        let stage = staged_and_applied();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            stage.path(),
        )
        .unwrap();

        assert!(!report.safe_exported);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "bundle_dir_equals_stage_dir"));
    }

    #[test]
    fn export_bundle_blocks_bundle_dir_equal_workspace() {
        let stage = staged_and_applied();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            Path::new(FULL_WS),
        )
        .unwrap();

        assert!(!report.safe_exported);
        assert!(report
            .blocked
            .iter()
            .any(|b| b.block_type == "bundle_dir_equals_workspace"));
    }

    #[test]
    fn export_bundle_does_not_modify_workspace_or_stage() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();

        let ws_file = Path::new(FULL_WS).join(TARGET_REL);
        let staged_file = stage.path().join(TARGET_REL);
        let ws_before = sha256_of(&ws_file);
        let staged_before = sha256_of(&staged_file);

        export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        assert_eq!(ws_before, sha256_of(&ws_file), "workspace file modified");
        assert_eq!(
            staged_before,
            sha256_of(&staged_file),
            "staged file modified"
        );
    }

    #[test]
    fn export_bundle_out_file_written_when_requested() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();

        // Simulate the CLI --out behavior.
        let out_path = bundle.path().join("export_report.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        assert!(out_path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["safeExported"], true);
        assert!(v["files"].is_array());
    }

    #[test]
    fn export_bundle_report_safe_true_when_successful() {
        let stage = staged_and_applied();
        let bundle = tempfile::TempDir::new().unwrap();
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();
        assert!(report.safe_exported);
        assert_eq!(report.summary.blocked_count, 0);
        assert!(report.summary.file_count >= 1);
    }

    #[test]
    fn export_bundle_report_safe_false_when_blocked() {
        let bundle = tempfile::TempDir::new().unwrap();
        let missing = bundle.path().join("nope_stage");
        let report = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            &missing,
            &bundle.path().join("out"),
        )
        .unwrap();
        assert!(!report.safe_exported);
        assert!(report.summary.blocked_count >= 1);
        assert!(report.manifest_path.is_none());
    }
}
