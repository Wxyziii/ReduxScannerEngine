#[cfg(test)]
mod tests {
    use crate::apply::text_apply::apply_patch_plan_to_stage;
    use crate::diff::model::{DiffLineType, DiffStatus};
    use crate::diff::preview::build_stage_diff_report;
    use crate::staging::stager::stage_patch_plan;
    use std::path::Path;

    const REPLACE_PLAN: &str = "../examples/patch_plans/valid_text_replace_patch.json";
    const FULL_WS: &str = "../examples/workspaces/update_rpf_fixture";

    fn stage_and_apply(plan: &str, dir: &tempfile::TempDir) {
        let report = stage_patch_plan(Path::new(plan), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(report.safe_to_stage, "staging failed: {:?}", report.blocked);
        let apply_report = apply_patch_plan_to_stage(Path::new(plan), dir.path()).unwrap();
        assert!(
            apply_report.safe_applied,
            "apply failed: {:?}",
            apply_report.blocked
        );
    }

    fn stage_only(plan: &str, dir: &tempfile::TempDir) {
        let report = stage_patch_plan(Path::new(plan), Path::new(FULL_WS), dir.path()).unwrap();
        assert!(report.safe_to_stage, "staging failed: {:?}", report.blocked);
    }

    // ── T0.4.5: diff-stage tests ───────────────────────────────────────────────

    #[test]
    fn diff_stage_detects_changed_file() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_and_apply(REPLACE_PLAN, &dir);

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        assert!(
            report.diffed_clean,
            "expected diffed_clean=true; blocked={:?}",
            report.blocked
        );
        let changed = report.files.iter().any(|f| f.changed);
        assert!(changed, "expected at least one changed file");
        let viz = report
            .files
            .iter()
            .find(|f| f.relative_path == "common/data/visualsettings.dat")
            .expect("visualsettings.dat should be in diff report");
        assert_eq!(viz.status, DiffStatus::Changed);
    }

    #[test]
    fn diff_stage_detects_unchanged_file() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_only(REPLACE_PLAN, &dir);

        // Stage without applying → staged content == workspace content.
        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        assert!(report.diffed_clean);
        let all_unchanged = report.files.iter().all(|f| !f.changed);
        assert!(all_unchanged, "expected all staged files to be unchanged");
        let viz = report
            .files
            .iter()
            .find(|f| f.relative_path == "common/data/visualsettings.dat")
            .expect("visualsettings.dat should be in diff report");
        assert_eq!(viz.status, DiffStatus::Unchanged);
    }

    #[test]
    fn diff_stage_reports_added_and_removed_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_and_apply(REPLACE_PLAN, &dir);

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        let viz = report
            .files
            .iter()
            .find(|f| f.relative_path == "common/data/visualsettings.dat")
            .unwrap();
        // text_replace changes exactly one line.
        assert_eq!(viz.lines_added, 1, "expected 1 line added");
        assert_eq!(viz.lines_removed, 1, "expected 1 line removed");
    }

    #[test]
    fn diff_stage_blocks_missing_original_file() {
        let dir = tempfile::TempDir::new().unwrap();
        // Create a file in stage_dir that does not exist in the workspace.
        let extra_dir = dir.path().join("custom/subdir");
        std::fs::create_dir_all(&extra_dir).unwrap();
        std::fs::write(extra_dir.join("noexist.dat"), b"some text\n").unwrap();

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        assert!(!report.diffed_clean, "expected diffed_clean=false");
        let has_missing = report
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_original");
        assert!(
            has_missing,
            "expected 'missing_original' block; got {:?}",
            report.blocked
        );
    }

    #[test]
    fn diff_stage_blocks_binary_file_preview() {
        let dir = tempfile::TempDir::new().unwrap();
        // Place binary (null-byte) content at a path that also exists in the workspace.
        let target_dir = dir.path().join("common/data");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("visualsettings.dat"), b"binary\x00data").unwrap();

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        assert!(!report.diffed_clean);
        let has_binary = report.blocked.iter().any(|b| b.block_type == "binary_file");
        assert!(
            has_binary,
            "expected 'binary_file' block; got {:?}",
            report.blocked
        );
    }

    #[test]
    fn diff_stage_preserves_relative_paths() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_only(REPLACE_PLAN, &dir);

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        for file in &report.files {
            assert!(
                !file.relative_path.starts_with('/'),
                "relative_path must not have a leading slash: {}",
                file.relative_path
            );
            assert!(
                !file.relative_path.contains('\\'),
                "relative_path must use forward slashes: {}",
                file.relative_path
            );
        }
        let viz = report
            .files
            .iter()
            .find(|f| f.relative_path == "common/data/visualsettings.dat");
        assert!(
            viz.is_some(),
            "expected 'common/data/visualsettings.dat' in results"
        );
    }

    #[test]
    fn diff_stage_does_not_modify_workspace_or_stage() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_and_apply(REPLACE_PLAN, &dir);

        let ws_file = Path::new(FULL_WS).join("common/data/visualsettings.dat");
        let staged_file = dir.path().join("common/data/visualsettings.dat");

        let ws_hash_before = {
            let data = std::fs::read(&ws_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };
        let staged_hash_before = {
            let data = std::fs::read(&staged_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };

        build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();

        let ws_hash_after = {
            let data = std::fs::read(&ws_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };
        let staged_hash_after = {
            let data = std::fs::read(&staged_file).unwrap();
            format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data))
        };

        assert_eq!(ws_hash_before, ws_hash_after, "workspace file was modified");
        assert_eq!(
            staged_hash_before, staged_hash_after,
            "staged file was modified"
        );
    }

    #[test]
    fn diff_stage_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_and_apply(REPLACE_PLAN, &dir);

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        let out_path = dir.path().join("diff_report.json");
        let json = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write(&out_path, &json).unwrap();

        assert!(out_path.exists());
        let content = std::fs::read_to_string(&out_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["diffedClean"], true);
        assert!(parsed["files"].is_array());
        assert!(parsed["summary"].is_object());
    }

    #[test]
    fn diff_stage_summary_counts_changed_files() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_and_apply(REPLACE_PLAN, &dir);

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        assert_eq!(
            report.summary.changed_count, 1,
            "expected 1 changed file in summary"
        );
        assert_eq!(report.summary.blocked_count, 0);
        assert!(report.summary.total_files >= 1);
    }

    #[test]
    fn diff_stage_preview_contains_context_add_remove_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        stage_and_apply(REPLACE_PLAN, &dir);

        let report = build_stage_diff_report(Path::new(FULL_WS), dir.path()).unwrap();
        let viz = report
            .files
            .iter()
            .find(|f| f.relative_path == "common/data/visualsettings.dat")
            .unwrap();

        assert!(!viz.hunks.is_empty(), "expected at least one hunk");
        let all_lines: Vec<&crate::diff::model::DiffLine> =
            viz.hunks.iter().flat_map(|h| h.lines.iter()).collect();

        let has_context = all_lines
            .iter()
            .any(|l| l.line_type == DiffLineType::Context);
        let has_add = all_lines.iter().any(|l| l.line_type == DiffLineType::Add);
        let has_remove = all_lines
            .iter()
            .any(|l| l.line_type == DiffLineType::Remove);

        assert!(has_context, "expected context lines in hunk");
        assert!(has_add, "expected add lines in hunk");
        assert!(has_remove, "expected remove lines in hunk");

        // Verify the actual line content.
        let add_content: Vec<&str> = all_lines
            .iter()
            .filter(|l| l.line_type == DiffLineType::Add)
            .map(|l| l.content.as_str())
            .collect();
        let remove_content: Vec<&str> = all_lines
            .iter()
            .filter(|l| l.line_type == DiffLineType::Remove)
            .map(|l| l.content.as_str())
            .collect();
        assert!(
            add_content.contains(&"Gamma 2.400000"),
            "expected 'Gamma 2.400000' as add line"
        );
        assert!(
            remove_content.contains(&"Gamma 2.200000"),
            "expected 'Gamma 2.200000' as remove line"
        );
    }
}
