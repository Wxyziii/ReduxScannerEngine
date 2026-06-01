#[cfg(test)]
mod tests {
    use crate::apply::text_apply::apply_patch_plan_to_stage;
    use crate::export::bundle::export_patch_bundle;
    use crate::rpf_writer::model::{GateSeverity, RpfWritePlanStatus};
    use crate::rpf_writer::plan::build_rpf_write_plan;
    use crate::staging::stager::stage_patch_plan;
    use std::path::{Path, PathBuf};

    const REPLACE_PLAN: &str = "../examples/patch_plans/valid_text_replace_patch.json";
    const FULL_WS: &str = "../examples/workspaces/update_rpf_fixture";

    /// A fake, non-existent target archive path. Planning never opens it.
    fn fake_target_rpf(dir: &Path) -> PathBuf {
        dir.join("fake_update.rpf")
    }

    /// Build a complete exported bundle in a fresh temp dir and return both dirs.
    fn make_bundle() -> (tempfile::TempDir, tempfile::TempDir) {
        let stage = tempfile::TempDir::new().unwrap();
        let bundle = tempfile::TempDir::new().unwrap();

        let s =
            stage_patch_plan(Path::new(REPLACE_PLAN), Path::new(FULL_WS), stage.path()).unwrap();
        assert!(s.safe_to_stage, "staging failed: {:?}", s.blocked);
        let a = apply_patch_plan_to_stage(Path::new(REPLACE_PLAN), stage.path()).unwrap();
        assert!(a.safe_applied, "apply failed: {:?}", a.blocked);
        let e = export_patch_bundle(
            Path::new(REPLACE_PLAN),
            Path::new(FULL_WS),
            stage.path(),
            bundle.path(),
        )
        .unwrap();
        assert!(e.safe_exported, "export failed: {:?}", e.blocked);

        (stage, bundle)
    }

    fn has_gate(plan: &crate::rpf_writer::model::RpfWritePlan, name: &str) -> bool {
        plan.safety_gates.iter().any(|g| g.gate == name)
    }
    fn gate_passed(plan: &crate::rpf_writer::model::RpfWritePlan, name: &str) -> Option<bool> {
        plan.safety_gates
            .iter()
            .find(|g| g.gate == name)
            .map(|g| g.passed)
    }

    #[test]
    fn rpf_write_plan_reads_bundle_manifest() {
        let (_stage, bundle) = make_bundle();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        assert_eq!(gate_passed(&plan, "bundle_manifest_present"), Some(true));
        assert_eq!(gate_passed(&plan, "bundle_safety_flags_valid"), Some(true));
        // The patched asset from the manifest is listed as a write target.
        assert!(plan
            .files_to_replace
            .iter()
            .any(|t| t.relative_path == "common/data/visualsettings.dat"));
        assert_eq!(plan.status, RpfWritePlanStatus::Planned);
    }

    #[test]
    fn rpf_write_plan_requires_bundle_manifest() {
        let empty = tempfile::TempDir::new().unwrap();
        let plan = build_rpf_write_plan(empty.path(), &fake_target_rpf(empty.path())).unwrap();

        assert_eq!(gate_passed(&plan, "bundle_manifest_present"), Some(false));
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_bundle_manifest"));
        assert_eq!(plan.status, RpfWritePlanStatus::Blocked);
    }

    #[test]
    fn rpf_write_plan_requires_files_dir() {
        let (_stage, bundle) = make_bundle();
        std::fs::remove_dir_all(bundle.path().join("files")).unwrap();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        assert_eq!(gate_passed(&plan, "files_present"), Some(false));
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_files_dir"));
        assert_eq!(plan.status, RpfWritePlanStatus::Blocked);
    }

    #[test]
    fn rpf_write_plan_requires_patch_plan() {
        let (_stage, bundle) = make_bundle();
        std::fs::remove_file(bundle.path().join("patch_plan.json")).unwrap();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        assert_eq!(gate_passed(&plan, "patch_plan_present"), Some(false));
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_patch_plan"));
    }

    #[test]
    fn rpf_write_plan_requires_diff_report() {
        let (_stage, bundle) = make_bundle();
        std::fs::remove_file(bundle.path().join("diff_report.json")).unwrap();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        assert_eq!(gate_passed(&plan, "diff_report_present"), Some(false));
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == "missing_diff_report"));
    }

    #[test]
    fn rpf_write_plan_blocks_non_rpf_target() {
        let (_stage, bundle) = make_bundle();
        let bad_target = bundle.path().join("update.zip");
        let plan = build_rpf_write_plan(bundle.path(), &bad_target).unwrap();

        assert_eq!(
            gate_passed(&plan, "target_archive_extension_is_rpf"),
            Some(false)
        );
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == "non_rpf_target"));
        assert_eq!(plan.status, RpfWritePlanStatus::Blocked);
    }

    #[test]
    fn rpf_write_plan_safe_to_write_false_by_default() {
        let (_stage, bundle) = make_bundle();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        assert!(!plan.safe_to_write, "safe_to_write must be false");
        assert!(!plan.real_writer_implemented);
        assert!(!plan.modifies_rpf);
    }

    #[test]
    fn rpf_write_plan_includes_backup_restore_hash_gates() {
        let (_stage, bundle) = make_bundle();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        assert!(has_gate(&plan, "backup_required"));
        assert!(has_gate(&plan, "restore_plan_required"));
        assert!(has_gate(&plan, "hash_verification_required"));
        assert!(has_gate(&plan, "manual_confirmation_required"));
        assert!(plan.backup_plan.required);
        assert!(plan.restore_plan.required);
        assert!(plan.hash_verification_required);
        assert!(plan.manual_confirmation_required);
    }

    #[test]
    fn rpf_write_plan_includes_real_writer_not_implemented_blocker() {
        let (_stage, bundle) = make_bundle();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        let g = plan
            .safety_gates
            .iter()
            .find(|g| g.gate == "real_rpf_writer_not_implemented")
            .expect("terminal gate must exist");
        assert!(!g.passed);
        assert_eq!(g.severity, GateSeverity::Blocking);
        assert!(plan
            .blocked
            .iter()
            .any(|b| b.block_type == "real_rpf_writer_not_implemented"));
    }

    #[test]
    fn rpf_write_plan_out_file_written_when_requested() {
        let (_stage, bundle) = make_bundle();
        let plan = build_rpf_write_plan(bundle.path(), &fake_target_rpf(bundle.path())).unwrap();

        let out_path = bundle.path().join("rpf_write_plan.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&plan).unwrap()).unwrap();
        assert!(out_path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(v["safeToWrite"], false);
        assert!(v["safetyGates"].is_array());
    }

    #[test]
    fn rpf_write_plan_does_not_modify_bundle_or_target() {
        let (_stage, bundle) = make_bundle();
        let target = fake_target_rpf(bundle.path());

        // Snapshot the bundle file tree (path -> sha256) before planning.
        fn snapshot(dir: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    snapshot(&p, out);
                } else if p.is_file() {
                    let data = std::fs::read(&p).unwrap();
                    out.push((
                        p.to_string_lossy().to_string(),
                        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&data)),
                    ));
                }
            }
        }
        let mut before = Vec::new();
        snapshot(bundle.path(), &mut before);
        before.sort();

        assert!(!target.exists(), "fake target must not exist before");
        let _ = build_rpf_write_plan(bundle.path(), &target).unwrap();

        let mut after = Vec::new();
        snapshot(bundle.path(), &mut after);
        after.sort();

        assert_eq!(before, after, "planning must not modify the bundle");
        assert!(
            !target.exists(),
            "planning must never create or write the target archive"
        );
    }
}
