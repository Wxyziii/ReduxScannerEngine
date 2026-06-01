use super::model::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST_VERSION: &str = "1";
const HASH_ALGORITHM: &str = "SHA-256";
/// Extensions we consider ordinary for a patched game asset.
const KNOWN_EXTENSIONS: &[&str] = &[
    "dat", "meta", "xml", "ymt", "ymap", "ytyp", "ydr", "ytd", "txt", "json", "gxt2",
];

// ── Bundle manifest (read-only view) ────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestView {
    #[serde(default)]
    bundle_format: Option<String>,
    #[serde(default)]
    files: Vec<ManifestFileView>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFileView {
    #[serde(default)]
    relative_path: String,
}

// ── Path safety ──────────────────────────────────────────────────────────────

/// Validate and normalize an archive-relative path. Returns the normalized
/// (forward-slash, case-preserved) path, or an error reason describing why it is
/// unsafe. Exact paths only — no suffix fallback, no traversal, no absolutes.
pub(crate) fn validate_archive_relative_path(raw: &str) -> Result<String, String> {
    let normalized = raw.replace('\\', "/");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err("empty path".to_string());
    }
    if trimmed.starts_with('/') {
        return Err("absolute path (leading separator)".to_string());
    }
    // Windows drive prefix, e.g. `C:/...`.
    if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
        return Err("absolute path (drive prefix)".to_string());
    }
    for component in trimmed.split('/') {
        if component.is_empty() {
            return Err("empty path component".to_string());
        }
        if component == ".." {
            return Err("parent traversal component".to_string());
        }
        if component == "." {
            return Err("current-dir component".to_string());
        }
    }
    Ok(trimmed.to_string())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn extension_of(rel: &str) -> String {
    Path::new(rel)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Recursively collect files under `current`, returning (absolute, path relative
/// to `base`) pairs with normalized forward-slash relative paths.
fn collect_files(base: &Path, current: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out);
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(base) {
                let normalized = rel
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches('/')
                    .to_string();
                out.push((path, normalized));
            }
        }
    }
}

fn empty_manifest(bundle_dir: &Path, target_rpf: Option<&Path>) -> RpfEntryManifest {
    RpfEntryManifest {
        manifest_version: MANIFEST_VERSION.to_string(),
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        target_rpf: target_rpf.map(|p| p.to_string_lossy().to_string()),
        entries: Vec::new(),
        modifies_rpf: false,
        native_parser_used: false,
        native_writer_used: false,
        external_tool_used: false,
        ready_for_write: false,
    }
}

fn blocked_report(
    bundle_dir: &Path,
    target_rpf: Option<&Path>,
    block_type: &str,
    path: String,
    reason: String,
) -> RpfEntryManifestReport {
    RpfEntryManifestReport {
        status: RpfEntryManifestStatus::Blocked,
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        target_rpf: target_rpf.map(|p| p.to_string_lossy().to_string()),
        manifest: empty_manifest(bundle_dir, target_rpf),
        blocked: vec![RpfEntryManifestBlockedItem {
            path,
            reason,
            block_type: block_type.to_string(),
        }],
        warnings: Vec::new(),
        summary: RpfEntryManifestSummary {
            entry_count: 0,
            duplicate_count: 0,
            unsafe_path_count: 0,
            blocked_count: 1,
            warning_count: 0,
        },
        modifies_rpf: false,
        native_parser_used: false,
        native_writer_used: false,
        external_tool_used: false,
        ready_for_write: false,
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Build a future-writer entry manifest from an exported patch bundle.
///
/// Read-only: reads `bundle_manifest.json` and walks `bundle_dir/files/`. NEVER
/// parses, opens, or modifies the target RPF, NEVER modifies the bundle, and
/// NEVER executes external tools. `ready_for_write` is always `false`.
pub fn build_rpf_entry_manifest(
    bundle_dir: &Path,
    target_rpf: Option<&Path>,
) -> Result<RpfEntryManifestReport, String> {
    // 1. bundle_manifest.json must exist and parse.
    let manifest_path = bundle_dir.join("bundle_manifest.json");
    if !manifest_path.is_file() {
        return Ok(blocked_report(
            bundle_dir,
            target_rpf,
            "missing_bundle_manifest",
            manifest_path.to_string_lossy().to_string(),
            "bundle_manifest.json not found".to_string(),
        ));
    }
    let manifest_content = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Cannot read bundle_manifest.json: {}", e))?;
    let manifest_view: ManifestView = serde_json::from_str(&manifest_content)
        .map_err(|e| format!("Cannot parse bundle_manifest.json: {}", e))?;
    let _ = manifest_view.bundle_format; // presence already validated by parse

    // 2. files/ must exist and be a non-empty directory.
    let files_dir = bundle_dir.join("files");
    if !files_dir.is_dir() {
        return Ok(blocked_report(
            bundle_dir,
            target_rpf,
            "missing_files_dir",
            files_dir.to_string_lossy().to_string(),
            "bundle files/ directory is missing".to_string(),
        ));
    }

    // 3. Walk files/ for the actual exported content (source of truth).
    let mut walked: Vec<(PathBuf, String)> = Vec::new();
    collect_files(&files_dir, &files_dir, &mut walked);
    walked.sort_by(|a, b| a.1.cmp(&b.1));
    if walked.is_empty() {
        return Ok(blocked_report(
            bundle_dir,
            target_rpf,
            "empty_files_dir",
            files_dir.to_string_lossy().to_string(),
            "bundle files/ directory is empty".to_string(),
        ));
    }

    let mut entries: Vec<RpfEntryManifestEntry> = Vec::new();
    let mut blocked: Vec<RpfEntryManifestBlockedItem> = Vec::new();
    let mut warnings: Vec<RpfEntryManifestWarning> = Vec::new();
    let mut unsafe_path_count = 0usize;

    for (abs, rel) in &walked {
        let (archive_relative_path, safe_path) = match validate_archive_relative_path(rel) {
            Ok(norm) => (norm, true),
            Err(reason) => {
                unsafe_path_count += 1;
                blocked.push(RpfEntryManifestBlockedItem {
                    path: rel.clone(),
                    reason: format!("unsafe archive path: {}", reason),
                    block_type: "unsafe_path".to_string(),
                });
                (rel.clone(), false)
            }
        };

        let bytes = fs::read(abs)
            .map_err(|e| format!("Cannot read bundle file {}: {}", abs.display(), e))?;
        let size = bytes.len() as u64;
        let sha256 = Some(sha256_hex(&bytes));
        let extension = extension_of(&archive_relative_path);

        if !extension.is_empty()
            && !KNOWN_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        {
            warnings.push(RpfEntryManifestWarning {
                path: archive_relative_path.clone(),
                message: format!("unusual extension '.{}'", extension),
                warning_type: "unusual_extension".to_string(),
            });
        }

        let bundle_file_relative_path = format!("files/{}", archive_relative_path);
        entries.push(RpfEntryManifestEntry {
            archive_relative_path,
            bundle_file_relative_path,
            bundle_file_absolute_path: abs.to_string_lossy().to_string(),
            extension,
            size_bytes: size,
            hash_algorithm: HASH_ALGORITHM.to_string(),
            sha256,
            replacement_source: "bundle/files".to_string(),
            would_replace_existing_entry: true,
            safe_path,
            operation_kind: "replace_file_planned".to_string(),
        });
    }

    // 4. Also validate paths declared in bundle_manifest.json, and detect
    //    duplicate declared targets (the walk itself cannot contain duplicates).
    let mut declared_counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in &manifest_view.files {
        match validate_archive_relative_path(&f.relative_path) {
            Ok(norm) => {
                *declared_counts.entry(norm).or_insert(0) += 1;
            }
            Err(reason) => {
                unsafe_path_count += 1;
                blocked.push(RpfEntryManifestBlockedItem {
                    path: f.relative_path.clone(),
                    reason: format!("unsafe declared path: {}", reason),
                    block_type: "unsafe_path".to_string(),
                });
            }
        }
    }
    let mut duplicate_count = 0usize;
    for (path, count) in &declared_counts {
        if *count > 1 {
            duplicate_count += 1;
            blocked.push(RpfEntryManifestBlockedItem {
                path: path.clone(),
                reason: format!("declared {} times in bundle manifest", count),
                block_type: "duplicate_target".to_string(),
            });
        }
    }

    let blocked_count = blocked.len();
    let warning_count = warnings.len();
    let entry_count = entries.len();

    let status = if blocked.is_empty() {
        RpfEntryManifestStatus::Built
    } else {
        RpfEntryManifestStatus::Blocked
    };

    let manifest = RpfEntryManifest {
        manifest_version: MANIFEST_VERSION.to_string(),
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        target_rpf: target_rpf.map(|p| p.to_string_lossy().to_string()),
        entries,
        modifies_rpf: false,
        native_parser_used: false,
        native_writer_used: false,
        external_tool_used: false,
        ready_for_write: false,
    };

    Ok(RpfEntryManifestReport {
        status,
        bundle_dir: bundle_dir.to_string_lossy().to_string(),
        target_rpf: target_rpf.map(|p| p.to_string_lossy().to_string()),
        manifest,
        blocked,
        warnings,
        summary: RpfEntryManifestSummary {
            entry_count,
            duplicate_count,
            unsafe_path_count,
            blocked_count,
            warning_count,
        },
        modifies_rpf: false,
        native_parser_used: false,
        native_writer_used: false,
        external_tool_used: false,
        ready_for_write: false,
    })
}
