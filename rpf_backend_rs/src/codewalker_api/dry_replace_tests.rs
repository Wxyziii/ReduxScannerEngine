#[cfg(test)]
mod dry_replace_tests {
    use crate::codewalker_api::dry_replace::build_codewalker_dry_replace_plan;
    use crate::codewalker_api::model::CodeWalkerDryReplacePlanStatus;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};

    const ARP: &str = "common/data/visualsettings.dat";
    const REL: &str = "files/common/data/visualsettings.dat";
    const CONTENT: &[u8] = b"visualsettings dry-replace fixture content\n";

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        format!("{:x}", h.finalize())
    }

    /// Lay out a bundle dir with `files/<ARP>` and return (bundle_dir, abs_path).
    fn write_bundle(dir: &Path) -> PathBuf {
        let abs = dir.join(REL);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, CONTENT).unwrap();
        abs
    }

    fn write_manifest(dir: &Path, abs: &Path, sha: Option<&str>) -> PathBuf {
        let mut entry = json!({
            "archiveRelativePath": ARP,
            "bundleFileRelativePath": REL,
            "bundleFileAbsolutePath": abs.display().to_string(),
            "sizeBytes": CONTENT.len(),
        });
        if let Some(s) = sha {
            entry["sha256"] = json!(s);
        }
        let report = json!({ "status": "built", "manifest": { "entries": [entry] } });
        let path = dir.join("rpf_entry_manifest_report.json");
        fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    fn write_resolve_resolved(dir: &Path) -> PathBuf {
        let report = json!({
            "resolvedTargets": [{
                "archiveRelativePath": ARP,
                "selectedCandidate": format!("update/{ARP}"),
                "matchType": "suffix"
            }],
            "unresolvedTargets": [],
            "ambiguousTargets": []
        });
        let path = dir.join("codewalker_resolve_targets.json");
        fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    /// A resolve report whose target was resolved via an archive-prefix
    /// preference (T0.6.13): the selected candidate carries the archive prefix.
    fn write_resolve_preferred_archive(dir: &Path) -> PathBuf {
        let report = json!({
            "preferredArchive": "update/update.rpf",
            "archivePrefixResolutionEnabled": true,
            "resolvedTargets": [{
                "archiveRelativePath": ARP,
                "selectedCandidate": format!("update/update.rpf/{ARP}"),
                "matchType": "suffix"
            }],
            "unresolvedTargets": [],
            "ambiguousTargets": []
        });
        let path = dir.join("codewalker_resolve_targets.json");
        fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    fn write_resolve_unresolved(dir: &Path) -> PathBuf {
        let report = json!({
            "resolvedTargets": [],
            "unresolvedTargets": [{ "archiveRelativePath": ARP }],
            "ambiguousTargets": []
        });
        let path = dir.join("codewalker_resolve_targets.json");
        fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    fn write_resolve_ambiguous(dir: &Path) -> PathBuf {
        let report = json!({
            "resolvedTargets": [],
            "unresolvedTargets": [{ "archiveRelativePath": ARP }],
            "ambiguousTargets": [{ "archiveRelativePath": ARP }]
        });
        let path = dir.join("codewalker_resolve_targets.json");
        fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    fn write_permission(dir: &Path) -> PathBuf {
        let report = json!({ "status": "token_issued", "writerAllowed": false });
        let path = dir.join("writer_permission_report.json");
        fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        path
    }

    /// Happy-path setup: resolved target, present bundle file, matching hash.
    fn happy(dir: &Path) -> (PathBuf, PathBuf) {
        let abs = write_bundle(dir);
        let manifest = write_manifest(dir, &abs, Some(&sha256_hex(CONTENT)));
        let resolve = write_resolve_resolved(dir);
        (manifest, resolve)
    }

    #[test]
    fn codewalker_dry_replace_reads_entry_manifest_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].archive_relative_path, ARP);
    }

    #[test]
    fn codewalker_dry_replace_reads_resolve_report() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(r.items[0].resolved_target.resolved);
        assert_eq!(
            r.items[0].codewalker_resolved_path.as_deref(),
            Some(format!("update/{ARP}").as_str())
        );
    }

    #[test]
    fn dry_replace_plan_emits_local_file_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        let p = r.items[0].planned_payload.as_ref().unwrap();
        assert!(!p.local_file_path.is_empty());
        assert!(p.local_file_path_exists);
        // The actual wire payload carries localFilePath + rpfFilePath.
        assert_eq!(p.actual_request_payload.local_file_path, p.local_file_path);
        assert_eq!(
            p.actual_request_payload.rpf_file_path,
            format!("update/{ARP}")
        );
    }

    #[test]
    fn dry_replace_plan_local_file_path_is_absolute() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        let p = r.items[0].planned_payload.as_ref().unwrap();
        assert!(p.local_file_path_is_absolute);
        assert!(Path::new(&p.local_file_path).is_absolute());
        assert!(Path::new(&p.actual_request_payload.local_file_path).is_absolute());
    }

    #[test]
    fn dry_replace_plan_blocks_if_bundle_file_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        // Manifest references a bundle file that was never written.
        let abs = dir.path().join(REL);
        let manifest = write_manifest(dir.path(), &abs, Some(&sha256_hex(CONTENT)));
        let resolve = write_resolve_resolved(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(r.items[0].planned_payload.is_none());
        assert_eq!(r.summary.planned_request_count, 0);
        assert!(r
            .blocked_items
            .iter()
            .any(|b| b.block_type == "item_not_ready_for_replace"));
    }

    #[test]
    fn dry_replace_plan_records_actual_request_payload() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        // Serialize and confirm the actualRequestPayload has exactly the API keys.
        let v: Value = serde_json::to_value(r.items[0].planned_payload.as_ref().unwrap()).unwrap();
        let actual = &v["actualRequestPayload"];
        assert!(actual["localFilePath"].is_string());
        assert!(actual["rpfFilePath"].is_string());
        assert_eq!(actual.as_object().unwrap().len(), 2);
        assert_eq!(v["apiContractName"], "codewalker_replace_file_v1");
        assert_eq!(v["requestSchemaValidated"], true);
    }

    #[test]
    fn dry_replace_plan_uses_discovered_replace_contract_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        let p = r.items[0].planned_payload.as_ref().unwrap();
        assert_eq!(p.endpoint, "/api/replace-file");
        assert_eq!(p.method, "POST");
        assert_eq!(p.codewalker_target_path, format!("update/{ARP}"));
        assert!(p.request_schema_validated);
    }

    #[test]
    fn dry_replace_plan_accepts_preferred_archive_resolved_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let abs = write_bundle(dir.path());
        let manifest = write_manifest(dir.path(), &abs, Some(&sha256_hex(CONTENT)));
        let resolve = write_resolve_preferred_archive(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(r.items[0].resolved_target.resolved);
        assert_eq!(
            r.items[0].codewalker_resolved_path.as_deref(),
            Some(format!("update/update.rpf/{ARP}").as_str())
        );
        assert!(r.summary.planned_request_count > 0);
        assert_eq!(r.status, CodeWalkerDryReplacePlanStatus::Planned);
    }

    #[test]
    fn codewalker_dry_replace_reads_permission_report_when_provided() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let perm = write_permission(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, Some(&perm))
            .unwrap();
        assert!(r.permission_report_path.is_some());
        let g = r
            .safety_gates
            .iter()
            .find(|g| g.name == "permission_report_loaded_or_not_required")
            .unwrap();
        assert!(g.passed);
    }

    #[test]
    fn codewalker_dry_replace_builds_planned_replace_payload() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert_eq!(r.planned_requests.len(), 1);
        let p = &r.planned_requests[0];
        assert_eq!(p.endpoint, "/api/replace-file");
        assert_eq!(p.method, "POST");
        assert!(p.dry_run_only);
        assert_eq!(p.archive_relative_path, ARP);
        assert_eq!(r.planned_endpoint, "/api/replace-file");
        assert_eq!(r.planned_http_method, "POST");
        assert!(r.items[0].planned_payload.is_some());
    }

    #[test]
    fn codewalker_dry_replace_verifies_bundle_file_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(r.items[0].bundle_file_exists);
        assert_eq!(r.items[0].bundle_file_size_bytes, CONTENT.len() as u64);
    }

    #[test]
    fn codewalker_dry_replace_computes_bundle_sha256() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert_eq!(
            r.items[0].bundle_file_sha256.as_deref(),
            Some(sha256_hex(CONTENT).as_str())
        );
        assert!(r.items[0].hash_matches_manifest);
    }

    #[test]
    fn codewalker_dry_replace_detects_hash_mismatch() {
        let dir = tempfile::TempDir::new().unwrap();
        let abs = write_bundle(dir.path());
        let manifest = write_manifest(dir.path(), &abs, Some(&"deadbeef".repeat(8)));
        let resolve = write_resolve_resolved(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.items[0].hash_matches_manifest);
        assert!(!r.items[0].valid_for_future_replace);
        assert!(r.planned_requests.is_empty());
    }

    #[test]
    fn codewalker_dry_replace_blocks_unresolved_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let abs = write_bundle(dir.path());
        let manifest = write_manifest(dir.path(), &abs, Some(&sha256_hex(CONTENT)));
        let resolve = write_resolve_unresolved(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.items[0].resolved_target.resolved);
        assert!(!r.items[0].valid_for_future_replace);
        assert_eq!(r.status, CodeWalkerDryReplacePlanStatus::Blocked);
    }

    #[test]
    fn codewalker_dry_replace_blocks_ambiguous_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let abs = write_bundle(dir.path());
        let manifest = write_manifest(dir.path(), &abs, Some(&sha256_hex(CONTENT)));
        let resolve = write_resolve_ambiguous(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(r.items[0].resolved_target.ambiguous);
        assert!(!r.items[0].valid_for_future_replace);
        let g = r
            .safety_gates
            .iter()
            .find(|g| g.name == "ambiguous_targets_blocked")
            .unwrap();
        assert!(!g.passed);
    }

    #[test]
    fn codewalker_dry_replace_blocks_missing_bundle_file() {
        let dir = tempfile::TempDir::new().unwrap();
        // Bundle dir + files/ exist but the file itself is absent.
        fs::create_dir_all(dir.path().join("files")).unwrap();
        let abs = dir.path().join(REL);
        let manifest = write_manifest(dir.path(), &abs, Some(&sha256_hex(CONTENT)));
        let resolve = write_resolve_resolved(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.items[0].bundle_file_exists);
        assert!(!r.items[0].valid_for_future_replace);
    }

    #[test]
    fn codewalker_dry_replace_ready_for_execution_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        // Even a fully valid plan stays not-ready in this milestone.
        assert_eq!(r.status, CodeWalkerDryReplacePlanStatus::Planned);
        assert!(!r.ready_for_execution);
        assert!(!r.summary.ready_for_execution);
    }

    #[test]
    fn codewalker_dry_replace_writer_allowed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.writer_allowed);
        assert!(!r.summary.writer_allowed);
        assert!(!r.can_write_archive);
    }

    #[test]
    fn codewalker_dry_replace_codewalker_execution_allowed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.codewalker_execution_allowed);
        assert!(!r.external_tool_executed);
    }

    #[test]
    fn codewalker_dry_replace_does_not_send_http_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.post_requests_sent);
        assert!(!r.get_requests_sent);
        assert!(!r.mutation_endpoints_called);
    }

    #[test]
    fn codewalker_dry_replace_does_not_use_post() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.post_requests_sent);
        // The POST method only appears as a MODELLED payload field, never sent.
        assert_eq!(r.planned_http_method, "POST");
    }

    #[test]
    fn codewalker_dry_replace_does_not_call_replace_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.replace_endpoint_called);
    }

    #[test]
    fn codewalker_dry_replace_does_not_call_import_endpoint() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.import_endpoint_called);
    }

    #[test]
    fn codewalker_dry_replace_does_not_call_reload_services() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.reload_services_called);
    }

    #[test]
    fn codewalker_dry_replace_does_not_call_set_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert!(!r.set_config_called);
    }

    #[test]
    fn codewalker_dry_replace_null_adapter_still_active() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        assert_eq!(r.active_adapter_name, "null_rpf_adapter");
        let g = r
            .safety_gates
            .iter()
            .find(|g| g.name == "null_adapter_still_active")
            .unwrap();
        assert!(g.passed);
    }

    #[test]
    fn codewalker_dry_replace_out_file_written_when_requested() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let r = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        let out = dir.path().join("plan.json");
        fs::write(&out, serde_json::to_string_pretty(&r).unwrap()).unwrap();
        assert!(out.is_file());
        let v: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["dryRunOnly"], true);
        assert_eq!(v["readyForExecution"], false);
        assert_eq!(v["writerAllowed"], false);
        assert_eq!(v["codewalkerExecutionAllowed"], false);
        assert_eq!(v["replaceEndpointCalled"], false);
        assert_eq!(v["postRequestsSent"], false);
        assert_eq!(v["modifiesArchive"], false);
        assert_eq!(v["plannedEndpoint"], "/api/replace-file");
        assert_eq!(v["activeAdapterName"], "null_rpf_adapter");
    }

    #[test]
    fn codewalker_dry_replace_does_not_modify_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let (manifest, resolve) = happy(dir.path());
        let before = fs::read(dir.path().join(REL)).unwrap();
        let count_before = fs::read_dir(dir.path()).unwrap().count();
        let _ = build_codewalker_dry_replace_plan(dir.path(), &manifest, &resolve, None).unwrap();
        let after = fs::read(dir.path().join(REL)).unwrap();
        let count_after = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(before, after);
        assert_eq!(count_before, count_after);
    }
}
