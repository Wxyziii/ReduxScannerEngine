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

#[derive(Debug, Clone, Serialize)]
struct EntryInfo {
    path: String,
    size: usize,
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

#[derive(Debug, Serialize)]
struct Report {
    schemaVersion: String,
    tool: ToolMetadata,
    timing: Timing,
    scan: ScanMetadata,
    rules: RulesMetadata,
    ok: bool,
    backend: String,
    cleanInput: String,
    moddedInput: String,
    keysPath: String,
    depth: usize,
    stats: Stats,
    warnings: Vec<Warning>,
    components: Vec<ComponentReport>,
    allChanges: Vec<Change>,
}

#[derive(Debug, Serialize)]
struct Stats {
    cleanEntries: usize,
    moddedEntries: usize,
    changedEntries: usize,
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
}

fn usage() {
    eprintln!(
        r#"rpf_backend_rs

Commands:
  compare --clean <clean.update.rpf> --modded <modded.update.rpf> --keys <keys_dir> --out <report.json> [--depth 2] [--mode fast|targeted|deep|full] [--all|--targets-only]
          [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  scan    --archive <update.rpf> --keys <keys_dir> --out <manifest.json> [--depth 2] [--mode fast|targeted|deep|full] [--all|--targets-only]
          [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  version

Notes:
  - This backend uses the rpf-archive crate.
  - Encrypted GTA V RPF7 requires a a valid keys directory.
  - Without keys, encrypted update.rpf cannot be read.
  - --all and --targets-only are deprecated; use --mode instead.
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
) -> Result<BTreeMap<String, EntryInfo>> {
    let temp = TempDir::new().context("failed to create temp directory")?;
    let mut map = BTreeMap::new();

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
    )?;

    Ok(map)
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
                    size: data.len(),
                    sha256,
                    source: archive_path.display().to_string(),
                },
            );
        }

        if options.allow_nested && depth > 0 && is_nested_rpf_target(&joined) {
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
                if c.size != m.size || c.sha256 != m.sha256 {
                    let reason = if c.size != m.size && c.sha256 != m.sha256 {
                        "size and sha256 differ"
                    } else if c.size != m.size {
                        "size differs"
                    } else {
                        "sha256 differs"
                    };

                    let mut meta = build_rich_metadata(&c.path, "modified", c.size, m.size);
                    if let Some(rules) = component_rules {
                        let matches = match_component_rules(&c.path, rules);
                        apply_component_rule_metadata(&mut meta, &matches);
                    }

                    changes.push(Change {
                        path: c.path.clone(),
                        status: "modified".to_string(),
                        cleanSize: c.size,
                        moddedSize: m.size,
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
                let mut meta = build_rich_metadata(&c.path, "removed", c.size, 0);
                if let Some(rules) = component_rules {
                    let matches = match_component_rules(&c.path, rules);
                    apply_component_rule_metadata(&mut meta, &matches);
                }
                changes.push(Change {
                    path: c.path.clone(),
                    status: "removed".to_string(),
                    cleanSize: c.size,
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
                let mut meta = build_rich_metadata(&m.path, "added", 0, m.size);
                if let Some(rules) = component_rules {
                    let matches = match_component_rules(&m.path, rules);
                    apply_component_rule_metadata(&mut meta, &matches);
                }
                changes.push(Change {
                    path: m.path.clone(),
                    status: "added".to_string(),
                    cleanSize: 0,
                    moddedSize: m.size,
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
    archive: &Path,
    keys_path: &Path,
    out: &Path,
    depth: usize,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    rules: &RulesMetadata,
    entries: &BTreeMap<String, EntryInfo>,
    warnings: &[Warning],
) -> Result<()> {
    #[derive(Serialize)]
    struct Manifest<'a> {
        schemaVersion: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        scan: &'a ScanMetadata,
        rules: &'a RulesMetadata,
        ok: bool,
        backend: &'a str,
        archive: String,
        keysPath: String,
        depth: usize,
        stats: ManifestStats,
        warnings: &'a [Warning],
        files: Vec<&'a EntryInfo>,
    }

    #[derive(Serialize)]
    struct ManifestStats {
        files: usize,
    }

    let manifest = Manifest {
        schemaVersion: SCHEMA_VERSION,
        tool,
        timing,
        scan,
        rules,
        ok: true,
        backend: "rpf_backend_rs/rpf-archive",
        archive: archive.display().to_string(),
        keysPath: keys_path.display().to_string(),
        depth,
        stats: ManifestStats {
            files: entries.len(),
        },
        warnings,
        files: entries.values().collect(),
    };

    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(out, json)?;
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

            let keys = GtaKeys::load_from_path(&keys_path).with_context(|| {
                format!("failed to load keys directory from {}", keys_path.display())
            })?;

            let mut warnings = Vec::new();
            if args.mode_was_explicit && args.deprecated_flag_used {
                push_deprecated_mode_warning(&mut warnings);
            }
            let rules = load_rules(&args, &mut warnings)?;
            let entries = scan_archive(
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
                &archive,
                &keys_path,
                &out,
                args.depth,
                &tool,
                &timing,
                &scan,
                &rules.metadata,
                &entries,
                &warnings,
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

            let keys = GtaKeys::load_from_path(&keys_path).with_context(|| {
                format!("failed to load keys directory from {}", keys_path.display())
            })?;

            let mut warnings = Vec::new();
            if args.mode_was_explicit && args.deprecated_flag_used {
                push_deprecated_mode_warning(&mut warnings);
            }
            let rules = load_rules(&args, &mut warnings)?;

            let clean_entries =
                scan_archive(
                    &clean,
                    &keys,
                    args.depth,
                    scan_options,
                    rules.target_rules.as_ref(),
                    &mut warnings,
                )
                    .with_context(|| format!("failed to scan clean archive {}", clean.display()))?;

            let modded_entries =
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

            let report = Report {
                schemaVersion: SCHEMA_VERSION.to_string(),
                tool,
                timing,
                scan,
                rules: rules.metadata,
                ok: true,
                backend: "rpf_backend_rs/rpf-archive".to_string(),
                cleanInput: clean.display().to_string(),
                moddedInput: modded.display().to_string(),
                keysPath: keys_path.display().to_string(),
                depth: args.depth,
                stats: Stats {
                    cleanEntries: clean_entries.len(),
                    moddedEntries: modded_entries.len(),
                    changedEntries: changes.len(),
                },
                warnings,
                components,
                allChanges: changes,
            };

            let json = serde_json::to_string_pretty(&report)?;
            fs::write(&out, json)?;

            println!("compare complete");
            println!("clean entries: {}", report.stats.cleanEntries);
            println!("modded entries: {}", report.stats.moddedEntries);
            println!("changed entries: {}", report.stats.changedEntries);
            println!("out: {}", out.display());

            println!("\nComponents:");
            for c in &report.components {
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
        _ => {
            usage();
            anyhow::bail!("unknown command: {}", args.command);
        }
    }

    Ok(())
}
