use anyhow::{Context, Result};
use rpf_archive::{GtaKeys, RpfFile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::TempDir;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const BACKEND_NAME: &str = "rpf_backend_rs";
const BACKEND_VERSION: &str = env!("CARGO_PKG_VERSION");
const SCHEMA_VERSION: &str = "2.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanMode {
    Fast,
    Targeted,
    Deep,
    Full,
}

impl ScanMode {
    fn as_str(self) -> &'static str {
        match self {
            ScanMode::Fast => "fast",
            ScanMode::Targeted => "targeted",
            ScanMode::Deep => "deep",
            ScanMode::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ScanMetadata {
    mode: String,
    depth: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ToolMetadata {
    name: String,
    version: String,
    backend: String,
    backendVersion: String,
    platform: String,
}

#[derive(Debug, Clone, Serialize)]
struct Timing {
    startedAt: String,
    finishedAt: String,
    durationMs: u64,
}

#[derive(Debug, Clone, Serialize)]
struct Warning {
    code: String,
    severity: String,
    path: String,
    message: String,
}

#[derive(Debug, Clone, Copy)]
struct ScanOptions {
    targets_only: bool,
    hash_entries: bool,
    allow_nested: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RulesMetadata {
    componentRulesSource: String,
    componentRulesPath: Option<String>,
    componentRulesVersion: String,
    targetRulesSource: String,
    targetRulesPath: Option<String>,
    targetRulesVersion: String,
    rulesDir: Option<String>,
    usedFallbackRules: bool,
}

#[derive(Debug, Deserialize)]
struct ComponentRulesFile {
    version: String,
    #[serde(default)]
    components: Vec<ComponentRuleFile>,
}

#[derive(Debug, Deserialize)]
struct ComponentRuleFile {
    id: String,
    name: String,
    #[serde(default)]
    editorNeeded: Vec<String>,
    #[serde(default)]
    risk: String,
    #[serde(default)]
    rules: Vec<ComponentMatchRuleFile>,
}

#[derive(Debug, Deserialize)]
struct ComponentMatchRuleFile {
    #[serde(rename = "type")]
    rule_type: String,
    value: String,
    #[serde(default)]
    confidence: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ComponentRules {
    version: String,
    components: Vec<ComponentRule>,
}

#[derive(Debug, Clone)]
struct ComponentRule {
    id: String,
    name: String,
    editor_needed: Vec<String>,
    risk: String,
    rules: Vec<ComponentMatchRule>,
}

#[derive(Debug, Clone)]
struct ComponentMatchRule {
    rule_type: RuleType,
    value: String,
    confidence: String,
    reason: String,
}

#[derive(Debug, Clone, Copy)]
enum RuleType {
    PathContains,
    BasenameEquals,
    BasenameStartsWith,
    ExtensionEquals,
    PathEndsWith,
}

#[derive(Debug, Clone)]
struct ComponentRuleMatch {
    component_id: String,
    component_name: String,
    confidence: String,
    reason: String,
    editor_needed: Vec<String>,
    risk: String,
}

#[derive(Debug, Deserialize)]
struct TargetRulesFile {
    version: String,
    #[serde(default)]
    targetExtensions: Vec<String>,
    #[serde(default)]
    targetPathContains: Vec<String>,
    #[serde(default)]
    targetBasenames: Vec<String>,
}

#[derive(Debug, Clone)]
struct TargetRules {
    version: String,
    target_extensions: BTreeSet<String>,
    target_path_contains: Vec<String>,
    target_basenames: BTreeSet<String>,
}

struct LoadedRules {
    component_rules: Option<ComponentRules>,
    target_rules: Option<TargetRules>,
    metadata: RulesMetadata,
}

#[derive(Debug, Clone)]
struct RichMetadata {
    extension: String,
    basename: String,
    parentPath: String,
    sizeDelta: i64,
    sizeDeltaPercent: Option<f64>,
    category: String,
    components: Vec<String>,
    editorNeeded: Vec<String>,
    risk: String,
    likelyPattern: String,
    confidence: String,
    warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntryInfo {
    path: String,
    name: String,
    extension: String,
    sizeBytes: usize,
    sha256: String,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
struct Change {
    path: String,
    status: String,
    cleanSize: usize,
    moddedSize: usize,
    cleanSha256: String,
    moddedSha256: String,
    extension: String,
    basename: String,
    parentPath: String,
    sizeDelta: i64,
    sizeDeltaPercent: Option<f64>,
    category: String,
    components: Vec<String>,
    editorNeeded: Vec<String>,
    risk: String,
    likelyPattern: String,
    confidence: String,
    warning: Option<String>,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct ComponentFileHit {
    path: String,
    status: String,
    confidence: String,
    reason: String,
    cleanSize: usize,
    moddedSize: usize,
    cleanSha256: String,
    moddedSha256: String,
    extension: String,
    basename: String,
    parentPath: String,
    sizeDelta: i64,
    sizeDeltaPercent: Option<f64>,
    category: String,
    components: Vec<String>,
    editorNeeded: Vec<String>,
    risk: String,
    likelyPattern: String,
    warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ComponentReport {
    id: String,
    name: String,
    status: String,
    confidence: String,
    files: Vec<ComponentFileHit>,
}

#[derive(Debug, Clone, Serialize)]
struct ArchiveIdentity {
    archivePath: String,
    archiveFileName: String,
    archiveSizeBytes: u64,
    archiveSha256: String,
}

#[derive(Debug, Default)]
struct ScanCounters {
    target_entries: usize,
    nested_archives_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
struct AnchorCheckResult {
    found: Vec<String>,
    missing: Vec<String>,
}

#[derive(Debug)]
struct Args {
    command: String,
    clean: Option<PathBuf>,
    modded: Option<PathBuf>,
    archive: Option<PathBuf>,
    keys: Option<PathBuf>,
    out: Option<PathBuf>,
    depth: usize,
    mode: ScanMode,
    mode_was_explicit: bool,
    deprecated_flag_used: bool,
    component_rules: Option<PathBuf>,
    target_rules: Option<PathBuf>,
    rules_dir: Option<PathBuf>,
    scanner_name: Option<String>,
    scanner_version: Option<String>,
    baseline: Option<PathBuf>,
}

fn usage() {
    eprintln!(
        r#"rpf_backend_rs

Commands:
  compare       --clean <clean.update.rpf> --modded <modded.update.rpf> --keys <keys_dir> --out <report.json>
                [--depth 2] [--mode fast|targeted|deep|full] [--all|--targets-only]
                [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  scan          --archive <update.rpf> --keys <keys_dir> --out <manifest.json>
                [--depth 2] [--mode fast|targeted|deep|full] [--all|--targets-only]
                [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  baseline-scan --archive <update.rpf> --keys <keys_dir> --out <baseline_output_dir>
                [--depth 2] [--mode full]
                [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  diff-against-baseline --modded <modded.update.rpf> --baseline <baseline_output_dir>
                --keys <keys_dir> --out <diff_output_dir>
                [--depth 2]
                [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  version

Notes:
  - This backend uses the rpf-archive crate.
  - Encrypted GTA V RPF7 requires a valid keys directory.
  - Without keys, encrypted update.rpf cannot be read.
  - --all and --targets-only are deprecated; use --mode instead.
  - baseline-scan writes: full_clean_manifest.json, full_clean_tree.json,
    baseline_update_tree_fingerprint.json, baseline_metadata.json into the --out folder.
  - diff-against-baseline reads baseline from --baseline folder and writes:
    full_modded_manifest.json, full_modded_tree.json,
    clean_vs_modded_diff.json, diff_summary.json into the --out folder.
"#
    );
}

fn parse_scan_mode(value: &str) -> Result<ScanMode> {
    match value {
        "fast" => Ok(ScanMode::Fast),
        "targeted" => Ok(ScanMode::Targeted),
        "deep" => Ok(ScanMode::Deep),
        "full" => Ok(ScanMode::Full),
        _ => anyhow::bail!("invalid --mode value: {}", value),
    }
}

fn resolve_scan_mode(explicit: Option<ScanMode>, deprecated: Option<ScanMode>) -> ScanMode {
    explicit.or(deprecated).unwrap_or(ScanMode::Targeted)
}

fn parse_args() -> Result<Args> {
    let mut it = env::args().skip(1);
    let command = it.next().unwrap_or_else(|| "".to_string());

    if command.is_empty() || command == "--help" || command == "-h" {
        usage();
        std::process::exit(0);
    }

    let mut args = Args {
        command,
        clean: None,
        modded: None,
        archive: None,
        keys: None,
        out: None,
        depth: 2,
        mode: ScanMode::Targeted,
        mode_was_explicit: false,
        deprecated_flag_used: false,
        component_rules: None,
        target_rules: None,
        rules_dir: None,
        scanner_name: None,
        scanner_version: None,
        baseline: None,
    };

    let mut explicit_mode: Option<ScanMode> = None;
    let mut deprecated_mode: Option<ScanMode> = None;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--clean" => {
                args.clean = Some(PathBuf::from(
                    it.next().context("missing value for --clean")?,
                ))
            }
            "--modded" => {
                args.modded = Some(PathBuf::from(
                    it.next().context("missing value for --modded")?,
                ))
            }
            "--archive" => {
                args.archive = Some(PathBuf::from(
                    it.next().context("missing value for --archive")?,
                ))
            }
            "--keys" => {
                args.keys = Some(PathBuf::from(
                    it.next().context("missing value for --keys")?,
                ))
            }
            "--out" => {
                args.out = Some(PathBuf::from(it.next().context("missing value for --out")?))
            }
            "--depth" => {
                let value = it.next().context("missing value for --depth")?;
                args.depth = value.parse::<usize>().context("invalid --depth value")?;
            }
            "--mode" => {
                let value = it.next().context("missing value for --mode")?;
                explicit_mode = Some(parse_scan_mode(&value)?);
                args.mode_was_explicit = true;
            }
            "--scanner-name" => {
                args.scanner_name = Some(it.next().context("missing value for --scanner-name")?)
            }
            "--scanner-version" => {
                args.scanner_version =
                    Some(it.next().context("missing value for --scanner-version")?)
            }
            "--component-rules" => {
                args.component_rules =
                    Some(PathBuf::from(it.next().context("missing value for --component-rules")?))
            }
            "--target-rules" => {
                args.target_rules =
                    Some(PathBuf::from(it.next().context("missing value for --target-rules")?))
            }
            "--rules-dir" => {
                args.rules_dir =
                    Some(PathBuf::from(it.next().context("missing value for --rules-dir")?))
            }
            "--baseline" => {
                args.baseline =
                    Some(PathBuf::from(it.next().context("missing value for --baseline")?))
            }
            "--all" => {
                args.deprecated_flag_used = true;
                deprecated_mode = Some(ScanMode::Full);
            }
            "--targets-only" => {
                args.deprecated_flag_used = true;
                deprecated_mode = Some(ScanMode::Targeted);
            }
            "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            _ => anyhow::bail!("unknown argument: {}", arg),
        }
    }

    args.mode = resolve_scan_mode(explicit_mode, deprecated_mode);

    Ok(args)
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_lowercase()
}

fn basename(path: &str) -> String {
    let p = normalize_path(path);
    match p.rsplit_once('/') {
        Some((_, b)) => b.to_string(),
        None => p,
    }
}

fn parent_path(path: &str) -> String {
    let p = normalize_path(path);
    match p.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

fn extension(path: &str) -> String {
    let b = basename(path);
    match b.rsplit_once('.') {
        Some((_, ext)) => ext.to_string(),
        None => String::new(),
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn hash_file(path: &Path) -> Result<String> {
    use std::io::{BufReader, Read};
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open file for hashing: {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("read error while hashing {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn build_archive_identity(path: &Path) -> Result<ArchiveIdentity> {
    let meta = fs::metadata(path)
        .with_context(|| format!("failed to stat archive: {}", path.display()))?;
    let sha256 = hash_file(path)?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(ArchiveIdentity {
        archivePath: path.display().to_string(),
        archiveFileName: file_name,
        archiveSizeBytes: meta.len(),
        archiveSha256: sha256,
    })
}

fn ensure_parent_dir(out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create output directory {}", parent.display())
            })?;
        }
    }
    Ok(())
}

fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

fn format_timestamp(timestamp: OffsetDateTime) -> Result<String> {
    timestamp
        .format(&Rfc3339)
        .context("failed to format UTC timestamp")
}

fn build_tool_metadata(args: &Args) -> ToolMetadata {
    ToolMetadata {
        name: args
            .scanner_name
            .clone()
            .unwrap_or_else(|| "redux_rpf_scanner".to_string()),
        version: args
            .scanner_version
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        backend: BACKEND_NAME.to_string(),
        backendVersion: BACKEND_VERSION.to_string(),
        platform: platform_name().to_string(),
    }
}

fn build_scan_metadata(args: &Args) -> ScanMetadata {
    ScanMetadata {
        mode: args.mode.as_str().to_string(),
        depth: args.depth,
    }
}

fn is_text_candidate(path: &str) -> bool {
    let ext = extension(path);
    matches!(
        ext.as_str(),
        "xml" | "dat" | "meta" | "ymt" | "cfg" | "ini" | "txt" | "json" | "gxt2" | "nametable"
    )
}

fn top_level_folder(path: &str) -> Option<String> {
    let p = normalize_path(path);
    let seg = p.split('/').next()?;
    if seg.is_empty() {
        None
    } else {
        Some(seg.to_string())
    }
}

fn build_extension_histogram(entries: &BTreeMap<String, EntryInfo>) -> BTreeMap<String, usize> {
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries.values() {
        let key = if entry.extension.is_empty() {
            "(none)".to_string()
        } else {
            entry.extension.clone()
        };
        *hist.entry(key).or_insert(0) += 1;
    }
    hist
}

fn collect_top_level_folders(entries: &BTreeMap<String, EntryInfo>) -> Vec<String> {
    let mut folders: BTreeSet<String> = BTreeSet::new();
    for entry in entries.values() {
        if let Some(f) = top_level_folder(&entry.path) {
            folders.insert(f);
        }
    }
    folders.into_iter().collect()
}

const ANCHOR_PATHS: &[&str] = &[
    "common/",
    "common/data/",
    "common/data/timecycle/",
    "common/data/visualsettings.dat",
    "x64/",
    "dlc_patch/",
];

const ANCHOR_BASENAMES: &[&str] = &["ptfx.rpf", "scaleform_minimap.rpf"];

fn check_anchor_paths(entries: &BTreeMap<String, EntryInfo>) -> AnchorCheckResult {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    'outer: for anchor in ANCHOR_PATHS {
        let trimmed = anchor.trim_end_matches('/');
        for key in entries.keys() {
            if key == trimmed || key.starts_with(*anchor) {
                found.push(anchor.to_string());
                continue 'outer;
            }
        }
        missing.push(anchor.to_string());
    }

    for bname in ANCHOR_BASENAMES {
        if entries.values().any(|e| e.name == *bname) {
            found.push(bname.to_string());
        } else {
            missing.push(bname.to_string());
        }
    }

    AnchorCheckResult { found, missing }
}

fn compute_tree_fingerprint_sha256(entries: &BTreeMap<String, EntryInfo>) -> String {
    // BTreeMap already sorted by key; join "path:size\n" for deterministic hash
    let joined: String = entries
        .values()
        .map(|e| format!("{}:{}", e.path, e.sizeBytes))
        .collect::<Vec<_>>()
        .join("\n");
    sha256_hex(joined.as_bytes())
}

fn scan_options_for_mode(mode: ScanMode, for_compare: bool) -> ScanOptions {
    let targets_only = match mode {
        ScanMode::Full => false,
        ScanMode::Fast => false,
        ScanMode::Targeted | ScanMode::Deep => true,
    };

    let allow_nested = mode != ScanMode::Fast;
    let hash_entries = if for_compare {
        true
    } else {
        mode != ScanMode::Fast
    };

    ScanOptions {
        targets_only,
        hash_entries,
        allow_nested,
    }
}

fn push_warning(warnings: &mut Vec<Warning>, code: &str, path: &str, message: String) {
    warnings.push(Warning {
        code: code.to_string(),
        severity: "warning".to_string(),
        path: path.to_string(),
        message,
    });
}

fn push_deprecated_mode_warning(warnings: &mut Vec<Warning>) {
    push_warning(
        warnings,
        "DEPRECATED_FLAG_IGNORED",
        "cli",
        "--all/--targets-only ignored because --mode was provided".to_string(),
    );
}

fn normalize_extension_value(value: &str) -> String {
    value.trim().trim_start_matches('.').to_lowercase()
}

fn normalize_confidence(value: Option<&str>) -> String {
    value.unwrap_or("medium").to_lowercase()
}

fn rule_type_from_str(value: &str) -> Option<RuleType> {
    match value {
        "path_contains" => Some(RuleType::PathContains),
        "basename_equals" => Some(RuleType::BasenameEquals),
        "basename_starts_with" => Some(RuleType::BasenameStartsWith),
        "extension_equals" => Some(RuleType::ExtensionEquals),
        "path_ends_with" => Some(RuleType::PathEndsWith),
        _ => None,
    }
}

fn normalize_rule_value(rule_type: RuleType, value: &str) -> String {
    match rule_type {
        RuleType::ExtensionEquals => normalize_extension_value(value),
        RuleType::BasenameEquals | RuleType::BasenameStartsWith => value.to_lowercase(),
        RuleType::PathContains | RuleType::PathEndsWith => normalize_path(value),
    }
}

fn rule_type_label(rule_type: RuleType) -> &'static str {
    match rule_type {
        RuleType::PathContains => "path_contains",
        RuleType::BasenameEquals => "basename_equals",
        RuleType::BasenameStartsWith => "basename_starts_with",
        RuleType::ExtensionEquals => "extension_equals",
        RuleType::PathEndsWith => "path_ends_with",
    }
}

fn build_component_rules(
    raw: ComponentRulesFile,
    source_path: &str,
    warnings: &mut Vec<Warning>,
) -> ComponentRules {
    let mut components = Vec::new();

    for component in raw.components {
        let mut rules = Vec::new();

        for rule in component.rules {
            let Some(rule_type) = rule_type_from_str(&rule.rule_type) else {
                push_warning(
                    warnings,
                    "RULE_UNSUPPORTED_TYPE",
                    source_path,
                    format!("Unsupported rule type: {}", rule.rule_type),
                );
                continue;
            };

            let normalized_value = normalize_rule_value(rule_type, &rule.value);
            if normalized_value.is_empty() {
                continue;
            }

            let confidence = normalize_confidence(rule.confidence.as_deref());
            let reason = rule
                .reason
                .unwrap_or_else(|| format!("Matched rule {} {}", rule.rule_type, rule.value));

            rules.push(ComponentMatchRule {
                rule_type,
                value: normalized_value,
                confidence,
                reason,
            });
        }

        components.push(ComponentRule {
            id: component.id,
            name: component.name,
            editor_needed: component.editorNeeded,
            risk: if component.risk.is_empty() {
                "unknown".to_string()
            } else {
                component.risk
            },
            rules,
        });
    }

    ComponentRules {
        version: raw.version,
        components,
    }
}

fn build_target_rules(raw: TargetRulesFile) -> TargetRules {
    let extensions = raw
        .targetExtensions
        .into_iter()
        .map(|ext| normalize_extension_value(&ext))
        .filter(|ext| !ext.is_empty())
        .collect::<BTreeSet<_>>();

    let path_contains = raw
        .targetPathContains
        .into_iter()
        .map(|value| normalize_path(&value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let basenames = raw
        .targetBasenames
        .into_iter()
        .map(|value| value.to_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();

    TargetRules {
        version: raw.version,
        target_extensions: extensions,
        target_path_contains: path_contains,
        target_basenames: basenames,
    }
}

fn load_component_rules_from_path(
    path: &Path,
    warnings: &mut Vec<Warning>,
) -> Result<ComponentRules> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read component rules file {}", path.display()))?;
    let raw: ComponentRulesFile = serde_json::from_str(&contents).with_context(|| {
        format!("failed to parse component rules file {}", path.display())
    })?;
    Ok(build_component_rules(
        raw,
        &path.display().to_string(),
        warnings,
    ))
}

fn load_target_rules_from_path(path: &Path) -> Result<TargetRules> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read target rules file {}", path.display()))?;
    let raw: TargetRulesFile = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse target rules file {}", path.display()))?;
    Ok(build_target_rules(raw))
}

fn load_rules(args: &Args, warnings: &mut Vec<Warning>) -> Result<LoadedRules> {
    if let Some(dir) = &args.rules_dir {
        if !dir.exists() {
            anyhow::bail!("rules dir does not exist: {}", dir.display());
        }
        if !dir.is_dir() {
            anyhow::bail!("rules dir is not a directory: {}", dir.display());
        }
    }

    let component_path = if let Some(path) = &args.component_rules {
        Some(path.clone())
    } else {
        args.rules_dir
            .as_ref()
            .map(|dir| dir.join("component_rules.json"))
    };
    let target_path = if let Some(path) = &args.target_rules {
        Some(path.clone())
    } else {
        args.rules_dir
            .as_ref()
            .map(|dir| dir.join("target_rules.json"))
    };

    let mut component_rules: Option<ComponentRules> = None;
    let mut target_rules: Option<TargetRules> = None;

    let mut component_source = "builtin".to_string();
    let mut target_source = "builtin".to_string();
    let mut component_path_meta: Option<String> = None;
    let mut target_path_meta: Option<String> = None;
    let mut component_version = "builtin".to_string();
    let mut target_version = "builtin".to_string();

    let mut rules_dir_partial = false;
    let mut rules_fallback_used = false;

    if let Some(path) = component_path {
        let explicit = args.component_rules.is_some();
        if !path.exists() {
            if explicit {
                anyhow::bail!("component rules file not found: {}", path.display());
            }
            rules_dir_partial = true;
            rules_fallback_used = true;
            push_warning(
                warnings,
                "RULE_FILE_NOT_FOUND",
                &path.display().to_string(),
                "component rules file not found; using builtin rules".to_string(),
            );
        } else {
            match load_component_rules_from_path(&path, warnings) {
                Ok(loaded) => {
                    component_version = loaded.version.clone();
                    component_path_meta = Some(path.display().to_string());
                    component_source = "file".to_string();
                    component_rules = Some(loaded);
                }
                Err(err) => {
                    if explicit {
                        return Err(err);
                    }
                    rules_dir_partial = true;
                    rules_fallback_used = true;
                    push_warning(
                        warnings,
                        "RULE_FILE_PARSE_FAILED",
                        &path.display().to_string(),
                        "component rules file failed to parse; using builtin rules".to_string(),
                    );
                }
            }
        }
    }

    if let Some(path) = target_path {
        let explicit = args.target_rules.is_some();
        if !path.exists() {
            if explicit {
                anyhow::bail!("target rules file not found: {}", path.display());
            }
            rules_dir_partial = true;
            rules_fallback_used = true;
            push_warning(
                warnings,
                "RULE_FILE_NOT_FOUND",
                &path.display().to_string(),
                "target rules file not found; using builtin rules".to_string(),
            );
        } else {
            match load_target_rules_from_path(&path) {
                Ok(loaded) => {
                    target_version = loaded.version.clone();
                    target_path_meta = Some(path.display().to_string());
                    target_source = "file".to_string();
                    target_rules = Some(loaded);
                }
                Err(err) => {
                    if explicit {
                        return Err(err);
                    }
                    rules_dir_partial = true;
                    rules_fallback_used = true;
                    push_warning(
                        warnings,
                        "RULE_FILE_PARSE_FAILED",
                        &path.display().to_string(),
                        "target rules file failed to parse; using builtin rules".to_string(),
                    );
                }
            }
        }
    }

    if args.rules_dir.is_some() && rules_dir_partial {
        push_warning(
            warnings,
            "RULES_DIR_PARTIAL",
            "rules-dir",
            "rules directory missing or invalid rule files; partial rules loaded".to_string(),
        );
    }

    if args.rules_dir.is_some() && rules_fallback_used {
        push_warning(
            warnings,
            "RULES_FALLBACK_USED",
            "rules",
            "using builtin fallback rules for missing/invalid rule files".to_string(),
        );
    }

    Ok(LoadedRules {
        component_rules,
        target_rules,
        metadata: RulesMetadata {
            componentRulesSource: component_source,
            componentRulesPath: component_path_meta,
            componentRulesVersion: component_version,
            targetRulesSource: target_source,
            targetRulesPath: target_path_meta,
            targetRulesVersion: target_version,
            rulesDir: args.rules_dir.as_ref().map(|p| p.display().to_string()),
            usedFallbackRules: rules_fallback_used,
        },
    })
}

fn compute_size_delta(clean_size: usize, modded_size: usize) -> (i64, Option<f64>) {
    let size_delta = modded_size as i64 - clean_size as i64;
    let percent = if clean_size > 0 {
        Some((size_delta as f64 / clean_size as f64) * 100.0)
    } else {
        None
    };
    (size_delta, percent)
}

fn build_rich_metadata(
    path: &str,
    status: &str,
    clean_size: usize,
    modded_size: usize,
) -> RichMetadata {
    let normalized = normalize_path(path);
    let base = basename(&normalized);
    let parent = parent_path(&normalized);
    let ext = extension(&normalized);
    let (size_delta, size_delta_percent) = compute_size_delta(clean_size, modded_size);

    let mut category = "unknown_binary".to_string();
    let mut risk = "unknown".to_string();
    let mut confidence = "low".to_string();
    let mut likely_pattern = match status {
        "added" => "asset_added",
        "removed" => "asset_removed",
        _ => "unknown_binary_change",
    }
    .to_string();
    let mut warning: Option<String> = None;

    let mut components = BTreeSet::new();
    let mut editor_needed = BTreeSet::new();

    let has_minimap = normalized.contains("minimap");
    let has_tracer = normalized.contains("tracer") || normalized.contains("ptfx_bullet_tracer");
    let has_blood = normalized.contains("blood")
        || normalized.contains("spray")
        || normalized.contains("hit_effect")
        || normalized.contains("hiteffect")
        || normalized.contains("ptfx_blood_spray");
    let is_scaleform_minimap =
        base == "scaleform_minimap.rpf" || normalized.contains("scaleform_minimap.rpf");
    let is_ptfx_container = base == "ptfx.rpf"
        || normalized.contains("effects/ptfx.rpf")
        || normalized.contains("ptfx.rpf");
    let is_core_ypt = base == "core.ypt" || normalized.ends_with("/core.ypt");
    let is_particle_container = ext == "ypt" || is_ptfx_container || is_core_ypt;
    let is_weather_xml = ext == "xml" && base.starts_with("w_");
    let is_timecycle_xml = ext == "xml" && is_timecycle_file(&normalized);
    let is_timecycle_mods_4 = normalized.contains("timecycle_mods_4");

    if ext == "xml" && (is_timecycle_xml || is_weather_xml) {
        category = if is_weather_xml {
            "weather_xml".to_string()
        } else {
            "timecycle_xml".to_string()
        };
        components.insert("sky_timecycle".to_string());
        components.insert("timecycle_weather".to_string());
        editor_needed.insert("xml_editor".to_string());
        risk = "low_medium".to_string();
        confidence = "high".to_string();
        if status == "modified" {
            likely_pattern = "timecycle_weather_restyle".to_string();
        }
        if is_timecycle_mods_4 {
            components.insert("kill_effect".to_string());
        }
    } else if base == "bloodfx.dat" {
        category = "blood_effect_config".to_string();
        components.insert("hit_effect".to_string());
        editor_needed.insert("dat_parser".to_string());
        risk = "medium".to_string();
        confidence = "high".to_string();
        if status == "modified" {
            likely_pattern = "config_value_change".to_string();
        }
    } else if ext == "gfx" {
        category = "scaleform_ui".to_string();
        editor_needed.insert("gfx_swf_converter".to_string());
        risk = "medium".to_string();
        warning = Some("Exact UI asset changes require a GFX/SWF analyzer.".to_string());
        confidence = if base == "minimap.gfx" {
            "high".to_string()
        } else {
            "medium".to_string()
        };
        if has_minimap {
            components.insert("minimap_hud".to_string());
        }
        if status == "modified" && has_minimap {
            likely_pattern = "minimap_hud_restyle".to_string();
        }
    } else if ext == "ytd" {
        category = "texture_dictionary".to_string();
        editor_needed.insert("ytd_texture_editor".to_string());
        risk = "medium".to_string();
        warning = Some("Exact texture-level changes require a YTD analyzer.".to_string());
        confidence = "medium".to_string();
        if has_minimap {
            components.insert("minimap_hud".to_string());
        }
        if status == "modified" {
            likely_pattern = "texture_dictionary_change".to_string();
        }
    } else if is_particle_container || normalized.contains("ptfx") {
        category = "particle_container".to_string();
        editor_needed.insert("ypt_particle_editor".to_string());
        risk = "medium_high".to_string();
        warning = Some("Exact particle-level changes require a YPT analyzer.".to_string());
        if normalized.contains("ptfx_bullet_tracer") || normalized.contains("ptfx_blood_spray") {
            confidence = "high".to_string();
        } else {
            confidence = "medium".to_string();
        }
        components.insert("tracer".to_string());
        components.insert("hit_effect".to_string());
        if status == "modified" {
            likely_pattern = "particle_container_reduction".to_string();
        }
    } else if is_scaleform_minimap {
        category = "scaleform_ui".to_string();
        editor_needed.insert("rpf_inspector".to_string());
        risk = "medium_high".to_string();
        warning = Some("Exact UI asset changes require a GFX/SWF analyzer.".to_string());
        confidence = "medium".to_string();
        components.insert("minimap_hud".to_string());
        if status == "modified" {
            likely_pattern = "minimap_hud_restyle".to_string();
        }
    } else if ext == "rpf" {
        category = "nested_rpf".to_string();
        editor_needed.insert("rpf_inspector".to_string());
        risk = "medium_high".to_string();
        warning = Some("Exact nested file changes require a deeper analyzer.".to_string());
        confidence = "low".to_string();
        if status == "modified" {
            likely_pattern = "nested_rpf_changed".to_string();
        }
    } else if ext == "meta" {
        category = "config_data".to_string();
        editor_needed.insert("meta_parser".to_string());
        risk = "medium".to_string();
        confidence = "low".to_string();
        if status == "modified" {
            likely_pattern = "config_value_change".to_string();
        }
    } else if ext == "ymt" {
        category = "config_data".to_string();
        editor_needed.insert("ymt_parser".to_string());
        risk = "medium".to_string();
        confidence = "low".to_string();
        if status == "modified" {
            likely_pattern = "config_value_change".to_string();
        }
    } else if has_minimap {
        category = "minimap_texture".to_string();
        editor_needed.insert("research_needed".to_string());
        risk = "medium".to_string();
        confidence = "medium".to_string();
        components.insert("minimap_hud".to_string());
        if status == "modified" {
            likely_pattern = "minimap_hud_restyle".to_string();
        }
    }

    if has_tracer {
        components.insert("tracer".to_string());
    }
    if has_blood {
        components.insert("hit_effect".to_string());
    }

    if components.is_empty() {
        components.insert("unknown".to_string());
    }

    if editor_needed.is_empty() {
        editor_needed.insert("research_needed".to_string());
    }

    RichMetadata {
        extension: ext,
        basename: base,
        parentPath: parent,
        sizeDelta: size_delta,
        sizeDeltaPercent: size_delta_percent,
        category,
        components: components.into_iter().collect(),
        editorNeeded: editor_needed.into_iter().collect(),
        risk,
        likelyPattern: likely_pattern,
        confidence,
        warning,
    }
}

impl TargetRules {
    fn is_target_relevant(&self, path: &str) -> bool {
        let normalized = normalize_path(path);
        let base = basename(&normalized);
        let ext = extension(&normalized);

        if self.target_extensions.contains(&ext) {
            return true;
        }

        if self.target_basenames.contains(&base) {
            return true;
        }

        for needle in &self.target_path_contains {
            if normalized.contains(needle) {
                return true;
            }
        }

        false
    }
}

fn is_timecycle_file(path: &str) -> bool {
    let p = normalize_path(path);
    let b = basename(&p);

    if p.contains("common/data/timecycle/") && p.ends_with(".xml") {
        return true;
    }

    matches!(
        b.as_str(),
        "timecycle_mods_1.xml"
            | "timecycle_mods_2.xml"
            | "timecycle_mods_3.xml"
            | "timecycle_mods_4.xml"
            | "w_blizzard.xml"
            | "w_clear.xml"
            | "w_clearing.xml"
            | "w_clouds.xml"
            | "w_extrasunny.xml"
            | "w_foggy.xml"
            | "w_halloween.xml"
            | "w_neutral.xml"
            | "w_overcast.xml"
            | "w_rain.xml"
            | "w_smog.xml"
            | "w_snow.xml"
            | "w_snowlight.xml"
            | "w_thunder.xml"
            | "w_xmas.xml"
    )
}

fn is_target_relevant(path: &str) -> bool {
    let p = normalize_path(path);
    let b = basename(&p);

    if is_timecycle_file(&p) {
        return true;
    }

    if b == "scaleform_minimap.rpf"
        || b == "minimap.gfx"
        || p.contains("scaleform_minimap.rpf")
        || p.contains("minimap")
    {
        return true;
    }

    if b == "ptfx.rpf"
        || b == "core.ypt"
        || p.contains("ptfx.rpf")
        || p.contains("core.ypt")
        || p.contains("ptfx_bullet_tracer")
        || p.contains("ptfx_blood_spray")
        || p.contains("tracer")
        || p.contains("blood")
        || p.contains("spray")
        || p.contains("hit_effect")
    {
        return true;
    }

    false
}

fn is_target_relevant_with_rules(path: &str, target_rules: Option<&TargetRules>) -> bool {
    if let Some(rules) = target_rules {
        return rules.is_target_relevant(path);
    }
    is_target_relevant(path)
}

fn is_nested_rpf_target(path: &str) -> bool {
    let p = normalize_path(path);
    let b = basename(&p);

    b == "scaleform_minimap.rpf"
        || b == "ptfx.rpf"
        || p.ends_with("x64/patch/data/cdimages/scaleform_minimap.rpf")
        || p.ends_with("x64/patch/data/effects/ptfx.rpf")
}

fn scan_archive(
    archive_path: &Path,
    keys: &GtaKeys,
    depth: usize,
    options: ScanOptions,
    target_rules: Option<&TargetRules>,
    warnings: &mut Vec<Warning>,
) -> Result<(BTreeMap<String, EntryInfo>, ScanCounters)> {
    let temp = TempDir::new().context("failed to create temp directory")?;
    let mut map = BTreeMap::new();
    let mut counters = ScanCounters::default();

    scan_archive_inner(
        archive_path,
        keys,
        depth,
        options,
        target_rules,
        warnings,
        &mut map,
        "",
        temp.path(),
        &mut counters,
    )?;

    Ok((map, counters))
}

fn scan_archive_inner(
    archive_path: &Path,
    keys: &GtaKeys,
    depth: usize,
    options: ScanOptions,
    target_rules: Option<&TargetRules>,
    warnings: &mut Vec<Warning>,
    map: &mut BTreeMap<String, EntryInfo>,
    prefix: &str,
    temp_root: &Path,
    counters: &mut ScanCounters,
) -> Result<()> {
    let file = RpfFile::open(archive_path, Some(keys))
        .with_context(|| format!("failed to open RPF archive: {}", archive_path.display()))?;

    file.walk(Some(keys), &mut |path, data| {
        let joined = if prefix.is_empty() {
            normalize_path(path)
        } else {
            normalize_path(&format!("{}/{}", prefix.trim_matches('/'), path))
        };

        if !options.targets_only || is_target_relevant_with_rules(&joined, target_rules) {
            let key = normalize_path(&joined);
            let sha256 = if options.hash_entries {
                sha256_hex(&data)
            } else {
                String::new()
            };
            map.insert(
                key.clone(),
                EntryInfo {
                    path: joined.clone(),
                    name: basename(&joined),
                    extension: extension(&joined),
                    sizeBytes: data.len(),
                    sha256,
                    source: archive_path.display().to_string(),
                },
            );
            counters.target_entries += 1;
        }

        if options.allow_nested && depth > 0 && is_nested_rpf_target(&joined) {
            counters.nested_archives_opened += 1;
            let safe_name = sha256_hex(&data);
            let nested_path = temp_root.join(format!("nested_{}.rpf", safe_name));

            if let Err(e) = fs::write(&nested_path, &data) {
                push_warning(
                    warnings,
                    "UNKNOWN_WARNING",
                    &joined,
                    format!("failed to write nested RPF temp file: {}", e),
                );
                return;
            }

            if let Err(e) = scan_archive_inner(
                &nested_path,
                keys,
                depth - 1,
                options,
                target_rules,
                warnings,
                map,
                &joined,
                temp_root,
                counters,
            ) {
                push_warning(
                    warnings,
                    "NESTED_RPF_OPEN_FAILED",
                    &joined,
                    format!("failed to open nested RPF: {}", e),
                );
            }
        }
    })?;

    Ok(())
}

fn diff_maps(
    clean: &BTreeMap<String, EntryInfo>,
    modded: &BTreeMap<String, EntryInfo>,
    component_rules: Option<&ComponentRules>,
) -> Vec<Change> {
    let mut keys = BTreeSet::new();

    for k in clean.keys() {
        keys.insert(k.clone());
    }

    for k in modded.keys() {
        keys.insert(k.clone());
    }

    let mut changes = Vec::new();

    for key in keys {
        match (clean.get(&key), modded.get(&key)) {
            (Some(c), Some(m)) => {
                if c.sizeBytes != m.sizeBytes || c.sha256 != m.sha256 {
                    let reason = if c.sizeBytes != m.sizeBytes && c.sha256 != m.sha256 {
                        "size and sha256 differ"
                    } else if c.sizeBytes != m.sizeBytes {
                        "size differs"
                    } else {
                        "sha256 differs"
                    };

                    let mut meta = build_rich_metadata(&c.path, "modified", c.sizeBytes, m.sizeBytes);
                    if let Some(rules) = component_rules {
                        let matches = match_component_rules(&c.path, rules);
                        apply_component_rule_metadata(&mut meta, &matches);
                    }

                    changes.push(Change {
                        path: c.path.clone(),
                        status: "modified".to_string(),
                        cleanSize: c.sizeBytes,
                        moddedSize: m.sizeBytes,
                        cleanSha256: c.sha256.clone(),
                        moddedSha256: m.sha256.clone(),
                        extension: meta.extension,
                        basename: meta.basename,
                        parentPath: meta.parentPath,
                        sizeDelta: meta.sizeDelta,
                        sizeDeltaPercent: meta.sizeDeltaPercent,
                        category: meta.category,
                        components: meta.components,
                        editorNeeded: meta.editorNeeded,
                        risk: meta.risk,
                        likelyPattern: meta.likelyPattern,
                        confidence: meta.confidence,
                        warning: meta.warning,
                        reason: reason.to_string(),
                    });
                }
            }
            (Some(c), None) => {
                let mut meta = build_rich_metadata(&c.path, "removed", c.sizeBytes, 0);
                if let Some(rules) = component_rules {
                    let matches = match_component_rules(&c.path, rules);
                    apply_component_rule_metadata(&mut meta, &matches);
                }
                changes.push(Change {
                    path: c.path.clone(),
                    status: "removed".to_string(),
                    cleanSize: c.sizeBytes,
                    moddedSize: 0,
                    cleanSha256: c.sha256.clone(),
                    moddedSha256: String::new(),
                    extension: meta.extension,
                    basename: meta.basename,
                    parentPath: meta.parentPath,
                    sizeDelta: meta.sizeDelta,
                    sizeDeltaPercent: meta.sizeDeltaPercent,
                    category: meta.category,
                    components: meta.components,
                    editorNeeded: meta.editorNeeded,
                    risk: meta.risk,
                    likelyPattern: meta.likelyPattern,
                    confidence: meta.confidence,
                    warning: meta.warning,
                    reason: "file exists only in clean archive".to_string(),
                });
            }
            (None, Some(m)) => {
                let mut meta = build_rich_metadata(&m.path, "added", 0, m.sizeBytes);
                if let Some(rules) = component_rules {
                    let matches = match_component_rules(&m.path, rules);
                    apply_component_rule_metadata(&mut meta, &matches);
                }
                changes.push(Change {
                    path: m.path.clone(),
                    status: "added".to_string(),
                    cleanSize: 0,
                    moddedSize: m.sizeBytes,
                    cleanSha256: String::new(),
                    moddedSha256: m.sha256.clone(),
                    extension: meta.extension,
                    basename: meta.basename,
                    parentPath: meta.parentPath,
                    sizeDelta: meta.sizeDelta,
                    sizeDeltaPercent: meta.sizeDeltaPercent,
                    category: meta.category,
                    components: meta.components,
                    editorNeeded: meta.editorNeeded,
                    risk: meta.risk,
                    likelyPattern: meta.likelyPattern,
                    confidence: meta.confidence,
                    warning: meta.warning,
                    reason: "file exists only in modded archive".to_string(),
                });
            }
            (None, None) => {}
        }
    }

    changes
}

fn confidence_rank(value: &str) -> u8 {
    match value {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn risk_rank(value: &str) -> u8 {
    match value {
        "high" => 5,
        "medium_high" => 4,
        "medium" => 3,
        "low_medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn match_component_rules(path: &str, rules: &ComponentRules) -> Vec<ComponentRuleMatch> {
    let normalized = normalize_path(path);
    let base = basename(&normalized);
    let ext = extension(&normalized);
    let mut matches = Vec::new();

    for component in &rules.components {
        let mut best_rule: Option<&ComponentMatchRule> = None;

        for rule in &component.rules {
            let is_match = match rule.rule_type {
                RuleType::PathContains => normalized.contains(&rule.value),
                RuleType::BasenameEquals => base == rule.value,
                RuleType::BasenameStartsWith => base.starts_with(&rule.value),
                RuleType::ExtensionEquals => ext == rule.value,
                RuleType::PathEndsWith => normalized.ends_with(&rule.value),
            };

            if is_match {
                let replace = match best_rule {
                    None => true,
                    Some(current) => {
                        confidence_rank(&rule.confidence) > confidence_rank(&current.confidence)
                    }
                };
                if replace {
                    best_rule = Some(rule);
                }
            }
        }

        if let Some(rule) = best_rule {
            matches.push(ComponentRuleMatch {
                component_id: component.id.clone(),
                component_name: component.name.clone(),
                confidence: rule.confidence.clone(),
                reason: rule.reason.clone(),
                editor_needed: component.editor_needed.clone(),
                risk: component.risk.clone(),
            });
        }
    }

    matches
}

fn apply_component_rule_metadata(meta: &mut RichMetadata, matches: &[ComponentRuleMatch]) {
    if matches.is_empty() {
        return;
    }

    let mut components = BTreeSet::new();
    let mut editor_needed = BTreeSet::new();
    let mut best_confidence = meta.confidence.clone();
    let mut best_risk = meta.risk.clone();

    for rule_match in matches {
        components.insert(rule_match.component_id.clone());
        for editor in &rule_match.editor_needed {
            editor_needed.insert(editor.clone());
        }

        if confidence_rank(&rule_match.confidence) > confidence_rank(&best_confidence) {
            best_confidence = rule_match.confidence.clone();
        }

        if risk_rank(&rule_match.risk) > risk_rank(&best_risk) {
            best_risk = rule_match.risk.clone();
        }
    }

    meta.components = components.into_iter().collect();
    meta.editorNeeded = editor_needed.into_iter().collect();
    meta.confidence = best_confidence;
    meta.risk = best_risk;
}

fn add_hit(report: &mut ComponentReport, ch: &Change, confidence: &str, reason: &str) {
    report.status = "changed".to_string();

    if confidence_rank(confidence) > confidence_rank(&report.confidence) {
        report.confidence = confidence.to_string();
    }

    report.files.push(ComponentFileHit {
        path: ch.path.clone(),
        status: ch.status.clone(),
        confidence: confidence.to_string(),
        reason: reason.to_string(),
        cleanSize: ch.cleanSize,
        moddedSize: ch.moddedSize,
        cleanSha256: ch.cleanSha256.clone(),
        moddedSha256: ch.moddedSha256.clone(),
        extension: ch.extension.clone(),
        basename: ch.basename.clone(),
        parentPath: ch.parentPath.clone(),
        sizeDelta: ch.sizeDelta,
        sizeDeltaPercent: ch.sizeDeltaPercent,
        category: ch.category.clone(),
        components: ch.components.clone(),
        editorNeeded: ch.editorNeeded.clone(),
        risk: ch.risk.clone(),
        likelyPattern: ch.likelyPattern.clone(),
        warning: ch.warning.clone(),
    });
}

fn new_component(id: &str, name: &str) -> ComponentReport {
    ComponentReport {
        id: id.to_string(),
        name: name.to_string(),
        status: "unchanged".to_string(),
        confidence: "none".to_string(),
        files: Vec::new(),
    }
}

fn ensure_component_report<'a>(
    reports: &'a mut BTreeMap<String, ComponentReport>,
    id: &str,
    name: &str,
) -> &'a mut ComponentReport {
    reports
        .entry(id.to_string())
        .or_insert_with(|| new_component(id, name))
}

fn apply_fallback_classification(ch: &Change, reports: &mut BTreeMap<String, ComponentReport>) {
    let p = normalize_path(&ch.path);
    let b = basename(&p);

    // Minimap
    if b == "minimap.gfx" || p.ends_with("/minimap.gfx") {
        let report = ensure_component_report(reports, "minimap_hud", "Minimap / HUD");
        add_hit(report, ch, "high", "Exact minimap.gfx file changed.");
    } else if b == "scaleform_minimap.rpf" || p.contains("scaleform_minimap.rpf") {
        let report = ensure_component_report(reports, "minimap_hud", "Minimap / HUD");
        add_hit(
            report,
            ch,
            "medium",
            "scaleform_minimap.rpf container changed; minimap.gfx likely changed inside.",
        );
    } else if p.contains("minimap") {
        let report = ensure_component_report(reports, "minimap_hud", "Minimap / HUD");
        add_hit(report, ch, "medium", "Path contains minimap keyword.");
    }

    // Tracer / hit effect
    if p.contains("ptfx_bullet_tracer") {
        let report = ensure_component_report(reports, "tracer", "Tracer");
        add_hit(report, ch, "high", "Exact ptfx_bullet_tracer asset changed.");
    }

    if p.contains("ptfx_blood_spray") {
        let report = ensure_component_report(reports, "hit_effect", "Hit Effect / Blood Spray");
        add_hit(report, ch, "high", "Exact ptfx_blood_spray asset changed.");
    }

    if b == "core.ypt" || p.ends_with("/core.ypt") {
        let tracer = ensure_component_report(reports, "tracer", "Tracer");
        add_hit(
            tracer,
            ch,
            "medium",
            "core.ypt changed; tracer lives inside this particle container.",
        );
        let hit = ensure_component_report(reports, "hit_effect", "Hit Effect / Blood Spray");
        add_hit(
            hit,
            ch,
            "medium",
            "core.ypt changed; blood spray/hit effect lives inside this particle container.",
        );
    }

    if b == "ptfx.rpf" || p.contains("effects/ptfx.rpf") || p.contains("ptfx.rpf") {
        let tracer = ensure_component_report(reports, "tracer", "Tracer");
        add_hit(
            tracer,
            ch,
            "medium",
            "ptfx.rpf container changed; tracer/core.ypt likely changed inside.",
        );
        let hit = ensure_component_report(reports, "hit_effect", "Hit Effect / Blood Spray");
        add_hit(
            hit,
            ch,
            "medium",
            "ptfx.rpf container changed; hit effect/core.ypt likely changed inside.",
        );
    }

    if p.contains("tracer") {
        let report = ensure_component_report(reports, "tracer", "Tracer");
        add_hit(report, ch, "medium", "Path contains tracer keyword.");
    }

    if p.contains("blood")
        || p.contains("spray")
        || p.contains("hit_effect")
        || p.contains("hiteffect")
    {
        let report = ensure_component_report(reports, "hit_effect", "Hit Effect / Blood Spray");
        add_hit(
            report,
            ch,
            "medium",
            "Path contains blood/spray/hit effect keyword.",
        );
    }

    // Timecycle
    if is_timecycle_file(&p) {
        let report = ensure_component_report(reports, "sky_timecycle", "Sky / Timecycle");
        add_hit(report, ch, "high", "Timecycle/weather XML changed.");
    }

    if b == "timecycle_mods_4.xml" || p.ends_with("common/data/timecycle/timecycle_mods_4.xml")
    {
        let report =
            ensure_component_report(reports, "kill_effect", "Kill Effect / Damage Screen Effect");
        add_hit(
            report,
            ch,
            "high",
            "timecycle_mods_4.xml changed; this is your kill effect file.",
        );
    } else if p.contains("timecycle_mods_4") {
        let report =
            ensure_component_report(reports, "kill_effect", "Kill Effect / Damage Screen Effect");
        add_hit(report, ch, "medium", "Path references timecycle_mods_4.");
    }
}

fn classify(changes: &[Change], component_rules: Option<&ComponentRules>) -> Vec<ComponentReport> {
    let mut reports: BTreeMap<String, ComponentReport> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    if let Some(rules) = component_rules {
        for component in &rules.components {
            reports
                .entry(component.id.clone())
                .or_insert_with(|| new_component(&component.id, &component.name));
            order.push(component.id.clone());
        }
    }

    for (id, name) in [
        ("tracer", "Tracer"),
        ("hit_effect", "Hit Effect / Blood Spray"),
        ("sky_timecycle", "Sky / Timecycle"),
        ("kill_effect", "Kill Effect / Damage Screen Effect"),
        ("minimap_hud", "Minimap / HUD"),
    ] {
        if !reports.contains_key(id) {
            reports.insert(id.to_string(), new_component(id, name));
            order.push(id.to_string());
        }
    }

    for ch in changes {
        let mut matched = false;
        if let Some(rules) = component_rules {
            let matches = match_component_rules(&ch.path, rules);
            if !matches.is_empty() {
                matched = true;
                for rule_match in matches {
                    let report = ensure_component_report(
                        &mut reports,
                        &rule_match.component_id,
                        &rule_match.component_name,
                    );
                    add_hit(report, ch, &rule_match.confidence, &rule_match.reason);
                }
            }
        }

        if !matched {
            apply_fallback_classification(ch, &mut reports);
        }
    }

    let mut result = Vec::new();
    for id in order {
        if let Some(report) = reports.remove(&id) {
            result.push(report);
        }
    }
    for (_, report) in reports {
        result.push(report);
    }

    result
}

fn write_scan_manifest(
    archive_identity: &ArchiveIdentity,
    keys_path: &Path,
    out: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    rules: &RulesMetadata,
    entries: &BTreeMap<String, EntryInfo>,
    warnings: &[Warning],
    counters: &ScanCounters,
) -> Result<()> {
    #[derive(Serialize)]
    struct ScanBlock<'a> {
        mode: &'a str,
        depth: usize,
        archivePath: &'a str,
        archiveFileName: &'a str,
        archiveSizeBytes: u64,
        archiveSha256: &'a str,
        keysPathProvided: bool,
    }

    #[derive(Serialize)]
    struct ManifestStats {
        totalEntries: usize,
        scannedEntries: usize,
        targetEntries: usize,
        nestedArchivesOpened: usize,
        warnings: usize,
    }

    #[derive(Serialize)]
    struct Manifest<'a> {
        schemaVersion: &'a str,
        ok: bool,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        scan: ScanBlock<'a>,
        rules: &'a RulesMetadata,
        stats: ManifestStats,
        warnings: &'a [Warning],
        files: Vec<&'a EntryInfo>,
    }

    let all_files: Vec<&EntryInfo> = entries.values().collect();

    let manifest = Manifest {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        tool,
        timing,
        scan: ScanBlock {
            mode: &scan.mode,
            depth: scan.depth,
            archivePath: &archive_identity.archivePath,
            archiveFileName: &archive_identity.archiveFileName,
            archiveSizeBytes: archive_identity.archiveSizeBytes,
            archiveSha256: &archive_identity.archiveSha256,
            keysPathProvided: !keys_path.as_os_str().is_empty(),
        },
        rules,
        stats: ManifestStats {
            totalEntries: entries.len(),
            scannedEntries: entries.len(),
            targetEntries: counters.target_entries,
            nestedArchivesOpened: counters.nested_archives_opened,
            warnings: warnings.len(),
        },
        warnings,
        files: all_files,
    };

    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(out, json)?;
    Ok(())
}

fn write_full_clean_manifest(
    out_dir: &Path,
    archive_identity: &ArchiveIdentity,
    keys_path: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    rules: &RulesMetadata,
    entries: &BTreeMap<String, EntryInfo>,
    warnings: &[Warning],
    counters: &ScanCounters,
) -> Result<()> {
    #[derive(Serialize)]
    struct BaselineFileEntry<'a> {
        path: &'a str,
        name: &'a str,
        extension: &'a str,
        sizeBytes: usize,
        sha256: &'a str,
        source: &'a str,
        isTextCandidate: bool,
        isBinaryCandidate: bool,
    }

    #[derive(Serialize)]
    struct BaselineScanBlock<'a> {
        mode: &'a str,
        depth: usize,
        archivePath: &'a str,
        archiveFileName: &'a str,
        archiveSizeBytes: u64,
        archiveSha256: &'a str,
        keysPathProvided: bool,
    }

    #[derive(Serialize)]
    struct BaselineManifestStats {
        totalEntries: usize,
        scannedEntries: usize,
        targetEntries: usize,
        nestedArchivesOpened: usize,
        textCandidates: usize,
        binaryCandidates: usize,
        warnings: usize,
    }

    #[derive(Serialize)]
    struct BaselineManifest<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        scan: BaselineScanBlock<'a>,
        rules: &'a RulesMetadata,
        stats: BaselineManifestStats,
        warnings: &'a [Warning],
        files: Vec<BaselineFileEntry<'a>>,
    }

    let files: Vec<BaselineFileEntry> = entries
        .values()
        .map(|e| {
            let is_text = is_text_candidate(&e.path);
            BaselineFileEntry {
                path: &e.path,
                name: &e.name,
                extension: &e.extension,
                sizeBytes: e.sizeBytes,
                sha256: &e.sha256,
                source: &e.source,
                isTextCandidate: is_text,
                isBinaryCandidate: !is_text,
            }
        })
        .collect();

    let text_count = files.iter().filter(|f| f.isTextCandidate).count();

    let manifest = BaselineManifest {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "full_clean_manifest",
        tool,
        timing,
        scan: BaselineScanBlock {
            mode: &scan.mode,
            depth: scan.depth,
            archivePath: &archive_identity.archivePath,
            archiveFileName: &archive_identity.archiveFileName,
            archiveSizeBytes: archive_identity.archiveSizeBytes,
            archiveSha256: &archive_identity.archiveSha256,
            keysPathProvided: !keys_path.as_os_str().is_empty(),
        },
        rules,
        stats: BaselineManifestStats {
            totalEntries: entries.len(),
            scannedEntries: entries.len(),
            targetEntries: counters.target_entries,
            nestedArchivesOpened: counters.nested_archives_opened,
            textCandidates: text_count,
            binaryCandidates: entries.len() - text_count,
            warnings: warnings.len(),
        },
        warnings,
        files,
    };

    let out = out_dir.join("full_clean_manifest.json");
    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_full_clean_tree(
    out_dir: &Path,
    archive_identity: &ArchiveIdentity,
    tool: &ToolMetadata,
    scan: &ScanMetadata,
    entries: &BTreeMap<String, EntryInfo>,
) -> Result<()> {
    #[derive(Serialize)]
    struct ExtHistEntry {
        extension: String,
        count: usize,
    }

    #[derive(Serialize)]
    struct TreeIdentityBlock<'a> {
        archivePath: &'a str,
        archiveFileName: &'a str,
        archiveSizeBytes: u64,
        archiveSha256: &'a str,
    }

    #[derive(Serialize)]
    struct TreeStats {
        totalEntries: usize,
        nestedArchiveEntries: usize,
        textCandidateEntries: usize,
    }

    #[derive(Serialize)]
    struct CleanTree<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        archive: TreeIdentityBlock<'a>,
        mode: &'a str,
        depth: usize,
        stats: TreeStats,
        topLevelFolders: Vec<String>,
        extensionCounts: Vec<ExtHistEntry>,
        paths: Vec<&'a str>,
    }

    let top_folders = collect_top_level_folders(entries);
    let ext_hist = build_extension_histogram(entries);
    let ext_counts: Vec<ExtHistEntry> = ext_hist
        .into_iter()
        .map(|(extension, count)| ExtHistEntry { extension, count })
        .collect();

    let nested_count = entries.values().filter(|e| e.extension == "rpf").count();
    let text_count = entries.values().filter(|e| is_text_candidate(&e.path)).count();
    let paths: Vec<&str> = entries.values().map(|e| e.path.as_str()).collect();

    let tree = CleanTree {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "full_clean_tree",
        tool,
        archive: TreeIdentityBlock {
            archivePath: &archive_identity.archivePath,
            archiveFileName: &archive_identity.archiveFileName,
            archiveSizeBytes: archive_identity.archiveSizeBytes,
            archiveSha256: &archive_identity.archiveSha256,
        },
        mode: &scan.mode,
        depth: scan.depth,
        stats: TreeStats {
            totalEntries: entries.len(),
            nestedArchiveEntries: nested_count,
            textCandidateEntries: text_count,
        },
        topLevelFolders: top_folders,
        extensionCounts: ext_counts,
        paths,
    };

    let out = out_dir.join("full_clean_tree.json");
    let json = serde_json::to_string_pretty(&tree)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_baseline_fingerprint(
    out_dir: &Path,
    archive_identity: &ArchiveIdentity,
    tool: &ToolMetadata,
    scan: &ScanMetadata,
    entries: &BTreeMap<String, EntryInfo>,
) -> Result<()> {
    #[derive(Serialize)]
    struct ExtCount {
        extension: String,
        count: usize,
    }

    #[derive(Serialize)]
    struct FingerprintIdentity<'a> {
        archivePath: &'a str,
        archiveFileName: &'a str,
        archiveSizeBytes: u64,
        archiveSha256: &'a str,
    }

    #[derive(Serialize)]
    struct BaselineFingerprint<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        archive: FingerprintIdentity<'a>,
        mode: &'a str,
        depth: usize,
        totalPaths: usize,
        treeFingerprintSha256: String,
        topLevelFolders: Vec<String>,
        extensionHistogram: Vec<ExtCount>,
        anchorPathsFound: Vec<String>,
        anchorPathsMissing: Vec<String>,
    }

    let anchor_result = check_anchor_paths(entries);
    let fingerprint_sha256 = compute_tree_fingerprint_sha256(entries);
    let top_folders = collect_top_level_folders(entries);
    let ext_hist = build_extension_histogram(entries);
    let ext_counts: Vec<ExtCount> = ext_hist
        .into_iter()
        .map(|(extension, count)| ExtCount { extension, count })
        .collect();

    let fp = BaselineFingerprint {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "baseline_update_tree_fingerprint",
        tool,
        archive: FingerprintIdentity {
            archivePath: &archive_identity.archivePath,
            archiveFileName: &archive_identity.archiveFileName,
            archiveSizeBytes: archive_identity.archiveSizeBytes,
            archiveSha256: &archive_identity.archiveSha256,
        },
        mode: &scan.mode,
        depth: scan.depth,
        totalPaths: entries.len(),
        treeFingerprintSha256: fingerprint_sha256,
        topLevelFolders: top_folders,
        extensionHistogram: ext_counts,
        anchorPathsFound: anchor_result.found,
        anchorPathsMissing: anchor_result.missing,
    };

    let out = out_dir.join("baseline_update_tree_fingerprint.json");
    let json = serde_json::to_string_pretty(&fp)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_baseline_metadata(
    out_dir: &Path,
    archive_identity: &ArchiveIdentity,
    tool: &ToolMetadata,
    scan: &ScanMetadata,
    rules: &RulesMetadata,
    timing: &Timing,
    artifact_names: &[&str],
) -> Result<()> {
    #[derive(Serialize)]
    struct BaselineRulesRef<'a> {
        componentRulesPath: Option<&'a String>,
        targetRulesPath: Option<&'a String>,
        rulesDir: Option<&'a String>,
        usedFallbackRules: bool,
    }

    #[derive(Serialize)]
    struct BaselineMetadata<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        baselineArchiveHash: &'a str,
        baselineArchiveSizeBytes: u64,
        baselineArchiveFileName: &'a str,
        scannerName: &'a str,
        scannerVersion: &'a str,
        backendVersion: &'a str,
        scanMode: &'a str,
        depth: usize,
        createdAt: &'a str,
        rules: BaselineRulesRef<'a>,
        artifacts: &'a [&'a str],
        reusableWhen: &'static str,
    }

    let meta = BaselineMetadata {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "baseline_metadata",
        baselineArchiveHash: &archive_identity.archiveSha256,
        baselineArchiveSizeBytes: archive_identity.archiveSizeBytes,
        baselineArchiveFileName: &archive_identity.archiveFileName,
        scannerName: &tool.name,
        scannerVersion: &tool.version,
        backendVersion: &tool.backendVersion,
        scanMode: &scan.mode,
        depth: scan.depth,
        createdAt: &timing.finishedAt,
        rules: BaselineRulesRef {
            componentRulesPath: rules.componentRulesPath.as_ref(),
            targetRulesPath: rules.targetRulesPath.as_ref(),
            rulesDir: rules.rulesDir.as_ref(),
            usedFallbackRules: rules.usedFallbackRules,
        },
        artifacts: artifact_names,
        reusableWhen: "archive sha256, scanner version, schema version, and rules version all match",
    };

    let out = out_dir.join("baseline_metadata.json");
    let json = serde_json::to_string_pretty(&meta)?;
    fs::write(&out, json)?;
    Ok(())
}

// ── Baseline loading structs ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct BaselineManifestFile {
    files: Vec<EntryInfo>,
}

#[derive(Deserialize)]
struct BaselineMetadataFile {
    baselineArchiveHash: String,
    baselineArchiveSizeBytes: u64,
    baselineArchiveFileName: String,
}

fn load_baseline_manifest(baseline_dir: &Path) -> Result<BTreeMap<String, EntryInfo>> {
    let manifest_path = baseline_dir.join("full_clean_manifest.json");
    let contents = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read baseline manifest: {}", manifest_path.display()))?;
    let parsed: BaselineManifestFile = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse baseline manifest: {}", manifest_path.display()))?;
    let mut map = BTreeMap::new();
    for entry in parsed.files {
        map.insert(entry.path.clone(), entry);
    }
    Ok(map)
}

fn load_baseline_metadata_file(baseline_dir: &Path) -> Result<BaselineMetadataFile> {
    let meta_path = baseline_dir.join("baseline_metadata.json");
    let contents = fs::read_to_string(&meta_path)
        .with_context(|| format!("failed to read baseline metadata: {}", meta_path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse baseline metadata: {}", meta_path.display()))
}

// ── Diff write functions ──────────────────────────────────────────────────────

fn write_full_modded_manifest(
    out_dir: &Path,
    archive_identity: &ArchiveIdentity,
    keys_path: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    rules: &RulesMetadata,
    entries: &BTreeMap<String, EntryInfo>,
    warnings: &[Warning],
    counters: &ScanCounters,
) -> Result<()> {
    #[derive(Serialize)]
    struct ModdedFileEntry<'a> {
        path: &'a str,
        name: &'a str,
        extension: &'a str,
        sizeBytes: usize,
        sha256: &'a str,
        source: &'a str,
        isTextCandidate: bool,
        isBinaryCandidate: bool,
    }

    #[derive(Serialize)]
    struct ModdedScanBlock<'a> {
        mode: &'a str,
        depth: usize,
        archivePath: &'a str,
        archiveFileName: &'a str,
        archiveSizeBytes: u64,
        archiveSha256: &'a str,
        keysPathProvided: bool,
    }

    #[derive(Serialize)]
    struct ModdedManifestStats {
        totalEntries: usize,
        scannedEntries: usize,
        targetEntries: usize,
        nestedArchivesOpened: usize,
        textCandidates: usize,
        binaryCandidates: usize,
        warnings: usize,
    }

    #[derive(Serialize)]
    struct ModdedManifest<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        scan: ModdedScanBlock<'a>,
        rules: &'a RulesMetadata,
        stats: ModdedManifestStats,
        warnings: &'a [Warning],
        files: Vec<ModdedFileEntry<'a>>,
    }

    let files: Vec<ModdedFileEntry> = entries
        .values()
        .map(|e| {
            let is_text = is_text_candidate(&e.path);
            ModdedFileEntry {
                path: &e.path,
                name: &e.name,
                extension: &e.extension,
                sizeBytes: e.sizeBytes,
                sha256: &e.sha256,
                source: &e.source,
                isTextCandidate: is_text,
                isBinaryCandidate: !is_text,
            }
        })
        .collect();

    let text_count = files.iter().filter(|f| f.isTextCandidate).count();

    let manifest = ModdedManifest {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "full_modded_manifest",
        tool,
        timing,
        scan: ModdedScanBlock {
            mode: &scan.mode,
            depth: scan.depth,
            archivePath: &archive_identity.archivePath,
            archiveFileName: &archive_identity.archiveFileName,
            archiveSizeBytes: archive_identity.archiveSizeBytes,
            archiveSha256: &archive_identity.archiveSha256,
            keysPathProvided: !keys_path.as_os_str().is_empty(),
        },
        rules,
        stats: ModdedManifestStats {
            totalEntries: entries.len(),
            scannedEntries: entries.len(),
            targetEntries: counters.target_entries,
            nestedArchivesOpened: counters.nested_archives_opened,
            textCandidates: text_count,
            binaryCandidates: entries.len() - text_count,
            warnings: warnings.len(),
        },
        warnings,
        files,
    };

    let out = out_dir.join("full_modded_manifest.json");
    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_full_modded_tree(
    out_dir: &Path,
    archive_identity: &ArchiveIdentity,
    tool: &ToolMetadata,
    scan: &ScanMetadata,
    entries: &BTreeMap<String, EntryInfo>,
) -> Result<()> {
    #[derive(Serialize)]
    struct ExtHistEntry {
        extension: String,
        count: usize,
    }

    #[derive(Serialize)]
    struct TreeIdentityBlock<'a> {
        archivePath: &'a str,
        archiveFileName: &'a str,
        archiveSizeBytes: u64,
        archiveSha256: &'a str,
    }

    #[derive(Serialize)]
    struct TreeStats {
        totalEntries: usize,
        nestedArchiveEntries: usize,
        textCandidateEntries: usize,
    }

    #[derive(Serialize)]
    struct ModdedTree<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        archive: TreeIdentityBlock<'a>,
        mode: &'a str,
        depth: usize,
        stats: TreeStats,
        topLevelFolders: Vec<String>,
        extensionCounts: Vec<ExtHistEntry>,
        paths: Vec<&'a str>,
    }

    let top_folders = collect_top_level_folders(entries);
    let ext_hist = build_extension_histogram(entries);
    let ext_counts: Vec<ExtHistEntry> = ext_hist
        .into_iter()
        .map(|(extension, count)| ExtHistEntry { extension, count })
        .collect();

    let nested_count = entries.values().filter(|e| e.extension == "rpf").count();
    let text_count = entries.values().filter(|e| is_text_candidate(&e.path)).count();
    let paths: Vec<&str> = entries.values().map(|e| e.path.as_str()).collect();

    let tree = ModdedTree {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "full_modded_tree",
        tool,
        archive: TreeIdentityBlock {
            archivePath: &archive_identity.archivePath,
            archiveFileName: &archive_identity.archiveFileName,
            archiveSizeBytes: archive_identity.archiveSizeBytes,
            archiveSha256: &archive_identity.archiveSha256,
        },
        mode: &scan.mode,
        depth: scan.depth,
        stats: TreeStats {
            totalEntries: entries.len(),
            nestedArchiveEntries: nested_count,
            textCandidateEntries: text_count,
        },
        topLevelFolders: top_folders,
        extensionCounts: ext_counts,
        paths,
    };

    let out = out_dir.join("full_modded_tree.json");
    let json = serde_json::to_string_pretty(&tree)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_clean_vs_modded_diff(
    out_dir: &Path,
    clean_identity: &ArchiveIdentity,
    modded_identity: &ArchiveIdentity,
    clean_entry_count: usize,
    modded_entry_count: usize,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    rules: &RulesMetadata,
    changes: &[Change],
    components: &[ComponentReport],
    warnings: &[Warning],
    keys_path: &Path,
) -> Result<()> {
    #[derive(Serialize)]
    struct DiffScanBlock<'a> {
        mode: &'a str,
        depth: usize,
        clean: &'a ArchiveIdentity,
        modded: &'a ArchiveIdentity,
        keysPathProvided: bool,
    }

    #[derive(Serialize)]
    struct DiffStats {
        cleanEntries: usize,
        moddedEntries: usize,
        addedEntries: usize,
        removedEntries: usize,
        modifiedEntries: usize,
        componentReports: usize,
        warnings: usize,
    }

    #[derive(Serialize)]
    struct CleanVsModdedDiff<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        scan: DiffScanBlock<'a>,
        rules: &'a RulesMetadata,
        stats: DiffStats,
        warnings: &'a [Warning],
        components: &'a [ComponentReport],
        allChanges: &'a [Change],
    }

    let added = changes.iter().filter(|c| c.status == "added").count();
    let removed = changes.iter().filter(|c| c.status == "removed").count();
    let modified = changes.iter().filter(|c| c.status == "modified").count();

    let report = CleanVsModdedDiff {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "clean_vs_modded_diff",
        tool,
        timing,
        scan: DiffScanBlock {
            mode: &scan.mode,
            depth: scan.depth,
            clean: clean_identity,
            modded: modded_identity,
            keysPathProvided: !keys_path.as_os_str().is_empty(),
        },
        rules,
        stats: DiffStats {
            cleanEntries: clean_entry_count,
            moddedEntries: modded_entry_count,
            addedEntries: added,
            removedEntries: removed,
            modifiedEntries: modified,
            componentReports: components.len(),
            warnings: warnings.len(),
        },
        warnings,
        components,
        allChanges: changes,
    };

    let json = serde_json::to_string_pretty(&report)?;
    let out = out_dir.join("clean_vs_modded_diff.json");
    fs::write(&out, json)?;
    Ok(())
}

fn write_diff_summary(
    out_dir: &Path,
    baseline_dir: &Path,
    clean_identity: &ArchiveIdentity,
    modded_identity: &ArchiveIdentity,
    clean_entry_count: usize,
    modded_entry_count: usize,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    warnings: &[Warning],
    changes: &[Change],
    components: &[ComponentReport],
) -> Result<()> {
    #[derive(Serialize)]
    struct ComponentSummary {
        id: String,
        name: String,
        status: String,
        confidence: String,
        fileCount: usize,
    }

    #[derive(Serialize)]
    struct DiffSummaryStats {
        cleanEntries: usize,
        moddedEntries: usize,
        addedEntries: usize,
        removedEntries: usize,
        modifiedEntries: usize,
        totalChanges: usize,
        componentReports: usize,
        changedComponents: usize,
        warnings: usize,
    }

    #[derive(Serialize)]
    struct DiffSummaryIdentity<'a> {
        archiveFileName: &'a str,
        archiveSizeBytes: u64,
        archiveSha256: &'a str,
    }

    #[derive(Serialize)]
    struct DiffSummaryScan<'a> {
        mode: &'a str,
        depth: usize,
        cleanBaseline: DiffSummaryIdentity<'a>,
        moddedArchive: DiffSummaryIdentity<'a>,
    }

    #[derive(Serialize)]
    struct DiffSummary<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        baselineDir: String,
        scan: DiffSummaryScan<'a>,
        stats: DiffSummaryStats,
        componentsSummary: Vec<ComponentSummary>,
        artifacts: Vec<String>,
        warnings: &'a [Warning],
    }

    let added = changes.iter().filter(|c| c.status == "added").count();
    let removed = changes.iter().filter(|c| c.status == "removed").count();
    let modified = changes.iter().filter(|c| c.status == "modified").count();
    let changed_components = components.iter().filter(|c| c.status == "changed").count();

    let components_summary: Vec<ComponentSummary> = components
        .iter()
        .map(|c| ComponentSummary {
            id: c.id.clone(),
            name: c.name.clone(),
            status: c.status.clone(),
            confidence: c.confidence.clone(),
            fileCount: c.files.len(),
        })
        .collect();

    let summary = DiffSummary {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "diff_summary",
        tool,
        timing,
        baselineDir: baseline_dir.display().to_string(),
        scan: DiffSummaryScan {
            mode: &scan.mode,
            depth: scan.depth,
            cleanBaseline: DiffSummaryIdentity {
                archiveFileName: &clean_identity.archiveFileName,
                archiveSizeBytes: clean_identity.archiveSizeBytes,
                archiveSha256: &clean_identity.archiveSha256,
            },
            moddedArchive: DiffSummaryIdentity {
                archiveFileName: &modded_identity.archiveFileName,
                archiveSizeBytes: modded_identity.archiveSizeBytes,
                archiveSha256: &modded_identity.archiveSha256,
            },
        },
        stats: DiffSummaryStats {
            cleanEntries: clean_entry_count,
            moddedEntries: modded_entry_count,
            addedEntries: added,
            removedEntries: removed,
            modifiedEntries: modified,
            totalChanges: changes.len(),
            componentReports: components.len(),
            changedComponents: changed_components,
            warnings: warnings.len(),
        },
        componentsSummary: components_summary,
        artifacts: vec![
            "full_modded_manifest.json".to_string(),
            "full_modded_tree.json".to_string(),
            "clean_vs_modded_diff.json".to_string(),
            "diff_summary.json".to_string(),
        ],
        warnings,
    };

    let out = out_dir.join("diff_summary.json");
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(&out, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scan_mode_accepts_valid_values() {
        assert_eq!(parse_scan_mode("fast").unwrap(), ScanMode::Fast);
        assert_eq!(parse_scan_mode("targeted").unwrap(), ScanMode::Targeted);
        assert_eq!(parse_scan_mode("deep").unwrap(), ScanMode::Deep);
        assert_eq!(parse_scan_mode("full").unwrap(), ScanMode::Full);
    }

    #[test]
    fn parse_scan_mode_rejects_invalid_values() {
        assert!(parse_scan_mode("nope").is_err());
    }

    #[test]
    fn resolve_scan_mode_defaults_to_targeted() {
        assert_eq!(resolve_scan_mode(None, None), ScanMode::Targeted);
        assert_eq!(
            resolve_scan_mode(None, Some(ScanMode::Full)),
            ScanMode::Full
        );
        assert_eq!(
            resolve_scan_mode(Some(ScanMode::Deep), Some(ScanMode::Full)),
            ScanMode::Deep
        );
    }

    #[test]
    fn archive_identity_serializes_expected_fields() {
        let identity = ArchiveIdentity {
            archivePath: "examples/fixtures/clean_update.rpf".to_string(),
            archiveFileName: "clean_update.rpf".to_string(),
            archiveSizeBytes: 12345,
            archiveSha256: "abc123".to_string(),
        };
        let json = serde_json::to_string(&identity).unwrap();
        assert!(json.contains("\"archivePath\""));
        assert!(json.contains("\"archiveFileName\""));
        assert!(json.contains("\"archiveSizeBytes\""));
        assert!(json.contains("\"archiveSha256\""));
        assert!(!json.contains("password"));
        assert!(!json.contains("key"));
    }

    #[test]
    fn rules_metadata_has_new_fields() {
        let meta = RulesMetadata {
            componentRulesSource: "fallback".to_string(),
            componentRulesPath: None,
            componentRulesVersion: "1.0".to_string(),
            targetRulesSource: "fallback".to_string(),
            targetRulesPath: None,
            targetRulesVersion: "1.0".to_string(),
            rulesDir: Some("rules/".to_string()),
            usedFallbackRules: true,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"rulesDir\""));
        assert!(json.contains("\"usedFallbackRules\""));
        assert!(json.contains("true"));
    }

    #[test]
    fn entry_info_uses_size_bytes_field() {
        let entry = EntryInfo {
            path: "common/data/weather.xml".to_string(),
            name: "weather.xml".to_string(),
            extension: "xml".to_string(),
            sizeBytes: 4096,
            sha256: "deadbeef".to_string(),
            source: "update.rpf".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"sizeBytes\""));
        assert!(json.contains("\"name\""));
        assert!(json.contains("\"extension\""));
        assert!(!json.contains("\"size\""));
    }

    #[test]
    fn schema_version_is_2_0() {
        assert_eq!(SCHEMA_VERSION, "2.0");
    }

    fn make_test_entries() -> BTreeMap<String, EntryInfo> {
        let mut m = BTreeMap::new();
        for (path, size) in [
            ("common/data/timecycle/timecycle_mods_1.xml", 1024usize),
            ("common/data/visualsettings.dat", 512),
            ("x64/patch/data/effects/ptfx.rpf", 8192),
            ("x64/textures/some_texture.ytd", 4096),
            ("dlc_patch/v1.0.0/x64/levels/level.ymap", 256),
        ] {
            let e = EntryInfo {
                path: path.to_string(),
                name: basename(path),
                extension: extension(path),
                sizeBytes: size,
                sha256: sha256_hex(path.as_bytes()),
                source: "test_archive.rpf".to_string(),
            };
            m.insert(path.to_string(), e);
        }
        m
    }

    #[test]
    fn is_text_candidate_identifies_known_extensions() {
        assert!(is_text_candidate("common/data/weather.xml"));
        assert!(is_text_candidate("common/data/visualsettings.dat"));
        assert!(is_text_candidate("some/file.meta"));
        assert!(is_text_candidate("some/file.ymt"));
        assert!(!is_text_candidate("textures.ytd"));
        assert!(!is_text_candidate("ptfx.rpf"));
        assert!(!is_text_candidate("minimap.gfx"));
    }

    #[test]
    fn extension_histogram_counts_correctly() {
        let entries = make_test_entries();
        let hist = build_extension_histogram(&entries);
        assert_eq!(*hist.get("xml").unwrap(), 1);
        assert_eq!(*hist.get("dat").unwrap(), 1);
        assert_eq!(*hist.get("rpf").unwrap(), 1);
        assert_eq!(*hist.get("ytd").unwrap(), 1);
        assert_eq!(*hist.get("ymap").unwrap(), 1);
    }

    #[test]
    fn top_level_folders_extracts_unique_roots() {
        let entries = make_test_entries();
        let folders = collect_top_level_folders(&entries);
        assert!(folders.contains(&"common".to_string()));
        assert!(folders.contains(&"x64".to_string()));
        assert!(folders.contains(&"dlc_patch".to_string()));
        assert_eq!(folders.len(), 3);
    }

    #[test]
    fn anchor_paths_detection_finds_expected() {
        let entries = make_test_entries();
        let result = check_anchor_paths(&entries);
        assert!(result.found.contains(&"common/".to_string()));
        assert!(result.found.contains(&"x64/".to_string()));
        assert!(result.found.contains(&"dlc_patch/".to_string()));
        assert!(result.found.contains(&"ptfx.rpf".to_string()));
        assert!(result.missing.contains(&"scaleform_minimap.rpf".to_string()));
    }

    #[test]
    fn tree_fingerprint_is_deterministic() {
        let entries = make_test_entries();
        let h1 = compute_tree_fingerprint_sha256(&entries);
        let h2 = compute_tree_fingerprint_sha256(&entries);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // sha256 hex = 64 chars
        assert!(!h1.is_empty());
    }

    #[test]
    fn tree_fingerprint_changes_on_different_entries() {
        let entries1 = make_test_entries();
        let mut entries2 = make_test_entries();
        if let Some(e) = entries2.get_mut("common/data/visualsettings.dat") {
            e.sizeBytes = 9999;
        }
        let h1 = compute_tree_fingerprint_sha256(&entries1);
        let h2 = compute_tree_fingerprint_sha256(&entries2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn load_baseline_manifest_round_trips_entries() {
        // Extra fields like isTextCandidate must be gracefully ignored via Deserialize
        let json = r#"{
            "schemaVersion": "2.0",
            "ok": true,
            "artifactType": "full_clean_manifest",
            "files": [
                {
                    "path": "common/data/weather.xml",
                    "name": "weather.xml",
                    "extension": "xml",
                    "sizeBytes": 1024,
                    "sha256": "abc123",
                    "source": "test.rpf",
                    "isTextCandidate": true,
                    "isBinaryCandidate": false
                }
            ]
        }"#;
        let parsed: BaselineManifestFile = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.files[0].sizeBytes, 1024);
        assert_eq!(parsed.files[0].extension, "xml");
    }
}

fn main() -> Result<()> {
    let args = parse_args()?;

    match args.command.as_str() {
        "version" => {
            println!("{} {}", BACKEND_NAME, BACKEND_VERSION);
        }
        "scan" => {
            let tool = build_tool_metadata(&args);
            let scan = build_scan_metadata(&args);
            let scan_options = scan_options_for_mode(args.mode, false);
            let started_at = OffsetDateTime::now_utc();
            let start_instant = Instant::now();
            let archive = args.archive.clone().context("scan requires --archive")?;
            let keys_path = args.keys.clone().context("scan requires --keys")?;
            let out = args.out.clone().context("scan requires --out")?;
            ensure_parent_dir(&out)?;

            let archive_identity = build_archive_identity(&archive)?;

            let keys = GtaKeys::load_from_path(&keys_path).with_context(|| {
                format!("failed to load keys directory from {}", keys_path.display())
            })?;

            let mut warnings = Vec::new();
            if args.mode_was_explicit && args.deprecated_flag_used {
                push_deprecated_mode_warning(&mut warnings);
            }
            let rules = load_rules(&args, &mut warnings)?;
            let (entries, counters) = scan_archive(
                &archive,
                &keys,
                args.depth,
                scan_options,
                rules.target_rules.as_ref(),
                &mut warnings,
            )?;

            let timing = Timing {
                startedAt: format_timestamp(started_at)?,
                finishedAt: format_timestamp(OffsetDateTime::now_utc())?,
                durationMs: start_instant.elapsed().as_millis() as u64,
            };

            write_scan_manifest(
                &archive_identity,
                &keys_path,
                &out,
                &tool,
                &timing,
                &scan,
                &rules.metadata,
                &entries,
                &warnings,
                &counters,
            )?;

            println!("scan complete");
            println!("archive: {}", archive.display());
            println!("entries: {}", entries.len());
            println!("out: {}", out.display());
        }
        "compare" => {
            let tool = build_tool_metadata(&args);
            let scan = build_scan_metadata(&args);
            let scan_options = scan_options_for_mode(args.mode, true);
            let started_at = OffsetDateTime::now_utc();
            let start_instant = Instant::now();
            let clean = args.clean.clone().context("compare requires --clean")?;
            let modded = args.modded.clone().context("compare requires --modded")?;
            let keys_path = args.keys.clone().context("compare requires --keys")?;
            let out = args.out.clone().context("compare requires --out")?;
            ensure_parent_dir(&out)?;

            let clean_identity = build_archive_identity(&clean)?;
            let modded_identity = build_archive_identity(&modded)?;

            let keys = GtaKeys::load_from_path(&keys_path).with_context(|| {
                format!("failed to load keys directory from {}", keys_path.display())
            })?;

            let mut warnings = Vec::new();
            if args.mode_was_explicit && args.deprecated_flag_used {
                push_deprecated_mode_warning(&mut warnings);
            }
            let rules = load_rules(&args, &mut warnings)?;

            let (clean_entries, _clean_counters) =
                scan_archive(
                    &clean,
                    &keys,
                    args.depth,
                    scan_options,
                    rules.target_rules.as_ref(),
                    &mut warnings,
                )
                    .with_context(|| format!("failed to scan clean archive {}", clean.display()))?;

            let (modded_entries, _modded_counters) =
                scan_archive(
                    &modded,
                    &keys,
                    args.depth,
                    scan_options,
                    rules.target_rules.as_ref(),
                    &mut warnings,
                )
                    .with_context(|| {
                        format!("failed to scan modded archive {}", modded.display())
                    })?;

            let changes =
                diff_maps(&clean_entries, &modded_entries, rules.component_rules.as_ref());
            let components = classify(&changes, rules.component_rules.as_ref());

            let timing = Timing {
                startedAt: format_timestamp(started_at)?,
                finishedAt: format_timestamp(OffsetDateTime::now_utc())?,
                durationMs: start_instant.elapsed().as_millis() as u64,
            };

            let added: usize = changes.iter().filter(|c| c.status == "added").count();
            let removed: usize = changes.iter().filter(|c| c.status == "removed").count();
            let modified: usize = changes.iter().filter(|c| c.status == "modified").count();
            let unchanged = clean_entries.len().saturating_sub(removed + modified);

            #[derive(Serialize)]
            struct CompareScanBlock<'a> {
                mode: &'a str,
                depth: usize,
                clean: &'a ArchiveIdentity,
                modded: &'a ArchiveIdentity,
                keysPathProvided: bool,
            }

            #[derive(Serialize)]
            struct CompareStats {
                cleanEntries: usize,
                moddedEntries: usize,
                addedEntries: usize,
                removedEntries: usize,
                modifiedEntries: usize,
                unchangedEntries: usize,
                componentReports: usize,
                warnings: usize,
            }

            #[derive(Serialize)]
            struct CompareReport<'a> {
                schemaVersion: &'a str,
                ok: bool,
                tool: &'a ToolMetadata,
                timing: &'a Timing,
                scan: CompareScanBlock<'a>,
                rules: &'a RulesMetadata,
                stats: CompareStats,
                warnings: &'a [Warning],
                components: &'a [ComponentReport],
                allChanges: &'a [Change],
            }

            let report = CompareReport {
                schemaVersion: SCHEMA_VERSION,
                ok: true,
                tool: &tool,
                timing: &timing,
                scan: CompareScanBlock {
                    mode: &scan.mode,
                    depth: scan.depth,
                    clean: &clean_identity,
                    modded: &modded_identity,
                    keysPathProvided: !keys_path.as_os_str().is_empty(),
                },
                rules: &rules.metadata,
                stats: CompareStats {
                    cleanEntries: clean_entries.len(),
                    moddedEntries: modded_entries.len(),
                    addedEntries: added,
                    removedEntries: removed,
                    modifiedEntries: modified,
                    unchangedEntries: unchanged,
                    componentReports: components.len(),
                    warnings: warnings.len(),
                },
                warnings: &warnings,
                components: &components,
                allChanges: &changes,
            };

            let json = serde_json::to_string_pretty(&report)?;
            fs::write(&out, json)?;

            println!("compare complete");
            println!("clean entries: {}", clean_entries.len());
            println!("modded entries: {}", modded_entries.len());
            println!("added: {}  removed: {}  modified: {}", added, removed, modified);
            println!("out: {}", out.display());

            println!("\nComponents:");
            for c in &components {
                if c.status == "changed" {
                    println!(
                        "  {}: CHANGED [{}] ({} match(es))",
                        c.name,
                        c.confidence,
                        c.files.len()
                    );
                } else {
                    println!("  {}: unchanged", c.name);
                }
            }
        }
        "baseline-scan" => {
            let tool = build_tool_metadata(&args);
            let started_at = OffsetDateTime::now_utc();
            let start_instant = Instant::now();
            let archive = args.archive.clone().context("baseline-scan requires --archive")?;
            let keys_path = args.keys.clone().context("baseline-scan requires --keys")?;
            let out_dir = args.out.clone().context("baseline-scan requires --out (output folder)")?;

            fs::create_dir_all(&out_dir).with_context(|| {
                format!("failed to create baseline output dir: {}", out_dir.display())
            })?;

            let archive_identity = build_archive_identity(&archive)?;
            let keys = GtaKeys::load_from_path(&keys_path).with_context(|| {
                format!("failed to load keys directory from {}", keys_path.display())
            })?;

            let mut warnings = Vec::new();
            let rules = load_rules(&args, &mut warnings)?;

            // Always use Full mode for baseline scan to capture every entry
            let scan_options = scan_options_for_mode(ScanMode::Full, false);
            let scan = ScanMetadata {
                mode: ScanMode::Full.as_str().to_string(),
                depth: args.depth,
            };

            let (entries, counters) = scan_archive(
                &archive,
                &keys,
                args.depth,
                scan_options,
                rules.target_rules.as_ref(),
                &mut warnings,
            )?;

            let timing = Timing {
                startedAt: format_timestamp(started_at)?,
                finishedAt: format_timestamp(OffsetDateTime::now_utc())?,
                durationMs: start_instant.elapsed().as_millis() as u64,
            };

            const BASELINE_ARTIFACTS: &[&str] = &[
                "full_clean_manifest.json",
                "full_clean_tree.json",
                "baseline_update_tree_fingerprint.json",
                "baseline_metadata.json",
            ];

            write_full_clean_manifest(
                &out_dir,
                &archive_identity,
                &keys_path,
                &tool,
                &timing,
                &scan,
                &rules.metadata,
                &entries,
                &warnings,
                &counters,
            )?;
            write_full_clean_tree(&out_dir, &archive_identity, &tool, &scan, &entries)?;
            write_baseline_fingerprint(&out_dir, &archive_identity, &tool, &scan, &entries)?;
            write_baseline_metadata(
                &out_dir,
                &archive_identity,
                &tool,
                &scan,
                &rules.metadata,
                &timing,
                BASELINE_ARTIFACTS,
            )?;

            println!("baseline-scan complete");
            println!("archive: {}", archive.display());
            println!("entries: {}", entries.len());
            println!("out: {}", out_dir.display());
            for name in BASELINE_ARTIFACTS {
                println!("  artifact: {}", name);
            }
        }
        "diff-against-baseline" => {
            let tool = build_tool_metadata(&args);
            let started_at = OffsetDateTime::now_utc();
            let start_instant = Instant::now();
            let modded = args.modded.clone().context("diff-against-baseline requires --modded")?;
            let baseline_dir = args.baseline.clone().context("diff-against-baseline requires --baseline")?;
            let keys_path = args.keys.clone().context("diff-against-baseline requires --keys")?;
            let out_dir = args.out.clone().context("diff-against-baseline requires --out (output folder)")?;

            fs::create_dir_all(&out_dir).with_context(|| {
                format!("failed to create diff output dir: {}", out_dir.display())
            })?;

            // Load clean baseline
            let clean_entries = load_baseline_manifest(&baseline_dir)?;
            let baseline_meta = load_baseline_metadata_file(&baseline_dir)?;

            let clean_identity = ArchiveIdentity {
                archivePath: format!("(baseline: {})", baseline_dir.display()),
                archiveFileName: baseline_meta.baselineArchiveFileName.clone(),
                archiveSizeBytes: baseline_meta.baselineArchiveSizeBytes,
                archiveSha256: baseline_meta.baselineArchiveHash.clone(),
            };

            // Scan modded archive in full mode with hashing
            let modded_identity = build_archive_identity(&modded)?;
            let keys = GtaKeys::load_from_path(&keys_path).with_context(|| {
                format!("failed to load keys directory from {}", keys_path.display())
            })?;

            let mut warnings = Vec::new();
            let rules = load_rules(&args, &mut warnings)?;

            let scan_options = scan_options_for_mode(ScanMode::Full, true);
            let scan = ScanMetadata {
                mode: ScanMode::Full.as_str().to_string(),
                depth: args.depth,
            };

            let (modded_entries, modded_counters) = scan_archive(
                &modded,
                &keys,
                args.depth,
                scan_options,
                rules.target_rules.as_ref(),
                &mut warnings,
            )?;

            let timing = Timing {
                startedAt: format_timestamp(started_at)?,
                finishedAt: format_timestamp(OffsetDateTime::now_utc())?,
                durationMs: start_instant.elapsed().as_millis() as u64,
            };

            // Diff
            let changes = diff_maps(&clean_entries, &modded_entries, rules.component_rules.as_ref());
            let components = classify(&changes, rules.component_rules.as_ref());

            let clean_entry_count = clean_entries.len();
            let modded_entry_count = modded_entries.len();

            write_full_modded_manifest(
                &out_dir,
                &modded_identity,
                &keys_path,
                &tool,
                &timing,
                &scan,
                &rules.metadata,
                &modded_entries,
                &warnings,
                &modded_counters,
            )?;
            write_full_modded_tree(&out_dir, &modded_identity, &tool, &scan, &modded_entries)?;
            write_clean_vs_modded_diff(
                &out_dir,
                &clean_identity,
                &modded_identity,
                clean_entry_count,
                modded_entry_count,
                &tool,
                &timing,
                &scan,
                &rules.metadata,
                &changes,
                &components,
                &warnings,
                &keys_path,
            )?;
            write_diff_summary(
                &out_dir,
                &baseline_dir,
                &clean_identity,
                &modded_identity,
                clean_entry_count,
                modded_entry_count,
                &tool,
                &timing,
                &scan,
                &warnings,
                &changes,
                &components,
            )?;

            let added = changes.iter().filter(|c| c.status == "added").count();
            let removed = changes.iter().filter(|c| c.status == "removed").count();
            let modified = changes.iter().filter(|c| c.status == "modified").count();

            println!("diff-against-baseline complete");
            println!("modded: {}", modded.display());
            println!("baseline: {}", baseline_dir.display());
            println!("clean entries: {}", clean_entry_count);
            println!("modded entries: {}", modded_entry_count);
            println!("added: {}", added);
            println!("removed: {}", removed);
            println!("modified: {}", modified);
            println!("out: {}", out_dir.display());
        }
        _ => {
            usage();
            anyhow::bail!("unknown command: {}", args.command);
        }
    }

    Ok(())
}
