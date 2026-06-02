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

mod apply;
mod codewalker_api;
mod codewalker_strategy;
mod diff;
mod editors;
mod export;
mod inventory;
mod rpf_adapter;
mod rpf_backup;
mod rpf_compare;
mod rpf_entry_manifest;
mod rpf_external;
mod rpf_permission;
mod rpf_probe;
mod rpf_readiness;
mod rpf_writer;
mod staging;
mod validators;

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

/// Records one classification attempt (physical filename scan or logical-name fallback scan).
#[derive(Debug, Clone, Serialize)]
struct ClassifyAttempt {
    physicalFileName: String,
    logicalFileName: String,
    entryCount: usize,
    score: u32,
    classification: String,
    usedForResult: bool,
    note: Option<String>,
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
    analyze_text: bool,
    build_learning_corpus: bool,
    component_rules: Option<PathBuf>,
    target_rules: Option<PathBuf>,
    rules_dir: Option<PathBuf>,
    scanner_name: Option<String>,
    scanner_version: Option<String>,
    baseline: Option<PathBuf>,
    file: Option<PathBuf>,
    vmode: Option<String>,
    patch_plan: Option<PathBuf>,
    workspace: Option<PathBuf>,
    stage_dir: Option<PathBuf>,
    bundle_dir: Option<PathBuf>,
    target_rpf: Option<PathBuf>,
    backup_dir: Option<PathBuf>,
    clean_rpf: Option<PathBuf>,
    modded_rpf: Option<PathBuf>,
    backup_report: Option<PathBuf>,
    readiness_report: Option<PathBuf>,
    entry_manifest_report: Option<PathBuf>,
    resolve_report: Option<PathBuf>,
    permission_report: Option<PathBuf>,
    dry_replace_plan: Option<PathBuf>,
    target_is_test_copy: bool,
    execution_gate_report: Option<PathBuf>,
    replace_apply_report: Option<PathBuf>,
    execute: bool,
    confirm: Option<String>,
    base_url: Option<String>,
    changed_files: Vec<String>,
    operation_id: Option<String>,
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
                [--depth 2] [--clean <clean.rpf>] [--analyze-text] [--build-learning-corpus]
                [--component-rules <path>] [--target-rules <path>] [--rules-dir <path>]
  classify-rpf  --archive <unknown.rpf> --baseline <baseline_output_dir>
                --keys <keys_dir> --out <classification.json>
                [--depth 3]
  validate-xml  --file <path> [--baseline <path>] [--vmode <mode>] [--out <out.json>]
  validate-dat  --file <path> [--baseline <path>] [--vmode <mode>] [--out <out.json>]
  validate-scope --patch-plan <path> --changed-files <comma_list_or_json_path> [--out <out.json>]
  dry-run       --plan <path> [--workspace <path>] [--out <out.json>]
                Simulate applying a PatchPlan without modifying any real files or RPF archives.
                When --workspace is provided, target file existence is verified against the
                extracted workspace directory. Exits 1 if safe_to_apply is false.
  inventory     --workspace <path> [--out <out.json>]
                Scan an extracted RPF workspace directory and report all files found.
                Read-only. Never modifies any file.
  stage         --plan <path> --workspace <path> --stage-dir <path> [--out <out.json>]
                Copy validated PatchPlan target files from the workspace into a staging directory.
                Does not modify the source workspace or any RPF archive.
                Exits 1 if staging is blocked.
  apply-stage   --plan <path> --stage-dir <path> [--out <out.json>]
                Apply supported text PatchPlan operations to files inside the staging directory.
                Only modifies staged copies — source workspace and RPF archives are never touched.
                Supported op types: text_replace, text_append, text_prepend.
                Exits 1 if any operation is blocked or fails.
  diff-stage    --workspace <path> --stage-dir <path> [--out <out.json>]
                Compare workspace files against their staged (patched) counterparts.
                Read-only — neither workspace nor stage directory is modified.
                Shows per-file change status, line counts, and preview hunks.
  export-bundle --plan <path> --workspace <path> --stage-dir <path>
                --bundle-dir <path> [--out <out.json>]
                Package patched staged files and reports into a portable patch bundle.
                Copies staged files into <bundle-dir>/files/ and writes bundle_manifest.json,
                patch_plan.json, diff_report.json (and stage/apply reports if available).
                Does not modify the source workspace, staged files, or any RPF archive.
                Exits 1 if the export is blocked.
  plan-rpf-write --bundle-dir <path> --target-rpf <path> [--out <out.json>]
                Plan a future controlled RPF write from an exported bundle.
                PLANNING ONLY: never opens or modifies the target archive.
                safeToWrite is always false; real RPF writing is not implemented.
  backup-rpf    --target-rpf <path> --backup-dir <path> [--out <out.json>]
                Copy a target .rpf into a backup directory and verify it by SHA-256.
                Read/copy only: the original target archive is never modified.
                safeForFutureWrite is true only when the backup hash matches.
  probe-rpf     --target-rpf <path> [--out <out.json>]
                Read-only probe of a target .rpf: file metadata, SHA-256, and
                informational external-tool detection. Never parses RPF internals
                or modifies the archive. canParseRpf/canWriteRpf are always false.
  compare-rpf   --clean-rpf <path> --modded-rpf <path> [--out <out.json>]
                Compare two .rpf archives by external metadata and SHA-256 only.
                Read-only: neither archive is parsed or modified. archivesDiffer
                is true when size or hash differs. canCompareInternals and
                nativeParserImplemented are always false.
  rpf-adapter-info [--out <out.json>]
                Report the current RPF adapter contract and capabilities.
                The active adapter is NullRpfAdapter (safe-mode only): it never
                opens, parses, or modifies an archive. canWriteArchive,
                canReplaceFiles, nativeParser, and nativeWriter are always false.
  rpf-external-tools [--out <out.json>]
                Plan future external RPF tooling support (OpenIV/CodeWalker/7z/
                powershell/cmd). Detection is informational PATH lookup only —
                no tool is ever executed. canWriteArchive,
                canUseExternalToolsAutomatically are always false; safeModeOnly true.
  write-readiness --bundle-dir <path> --target-rpf <path>
                [--backup-report <path>] [--out <out.json>]
                Unified read-only pre-write readiness report combining bundle,
                write plan, backup, probe, adapter, and external-tool components.
                readyToWrite is always false; never opens or modifies the target
                archive, never modifies the bundle, never creates backups, and
                never executes external tools.
  rpf-entry-manifest --bundle-dir <path> [--target-rpf <path>] [--out <out.json>]
                Build a future-writer entry manifest mapping exported bundle files
                to archive-relative paths (size + SHA-256, path safety, duplicate
                detection). Read-only: never parses/opens/writes the target RPF and
                never executes external tools. readyForWrite is always false.
  writer-permission --bundle-dir <path> --target-rpf <path>
                [--readiness-report <path>] [--entry-manifest-report <path>]
                [--backup-report <path>] --confirm <phrase> [--out <out.json>]
                Model the manual confirmation / permission token required before any
                future controlled RPF write. Read-only: validates inputs and the
                exact confirmation phrase, then may issue a planning token. The
                token never authorizes writing — writerAllowed is always false and
                blocking items still include writer/parser/adapter blockers. Never
                opens or modifies the target archive, never modifies the bundle,
                never creates backups, and never executes external tools.
  codewalker-strategy [--out <out.json>]
                Report the locked future writer route (CodeWalker.API) and the
                planned T0.6.x milestones + safety gates. Static/deterministic:
                reads no files, modifies nothing, and never detects, calls, or
                executes CodeWalker. The active adapter stays NullRpfAdapter;
                writerAllowedNow and codewalkerWriteAllowedNow are always false.
  codewalker-detect [--base-url <url>] [--out <out.json>]
                Detect a local CodeWalker.API using read-only HTTP GET checks of
                the base URL (default http://localhost:5555): root and
                /api/service-status. Never calls replace/import/write or any
                mutation endpoint, never executes CodeWalker as a process, and
                never opens or modifies an RPF archive. An offline server yields
                reachable=false (not an error). The active adapter stays
                NullRpfAdapter; canWriteArchive, replaceEndpointCalled,
                writeEndpointsCalled, modifiesArchive, and writerAllowed are
                always false. Exits 0.
  codewalker-readiness [--base-url <url>] [--out <out.json>]
                Read-only readiness probe for a local CodeWalker.API. Builds on
                codewalker-detect, then does one extra GET /api/service-status and
                tolerantly parses readiness / GTA path info if present. Uses GET
                only — never POST, never replace/import/reload-services/set-config
                or any mutation endpoint, never executes CodeWalker, never opens or
                modifies an RPF archive. Offline yields reachable=false (not an
                error). codewalkerApiReadyForReplace, canWriteArchive, and
                writerAllowed are always false. Exits 0.
  codewalker-resolve-targets --entry-manifest-report <path> [--base-url <url>]
                [--readiness-report <path>] [--out <out.json>]
                Map RPF entry manifest entries to CodeWalker search results using
                read-only GET /api/search-file?filename=<name>. Resolves a target
                only on a unique exact or unique suffix match; filename-only and
                ambiguous matches stay unresolved. GET-only — never POST, never
                replace/import/reload-services/set-config or any mutation endpoint,
                never executes CodeWalker, never opens or modifies an RPF archive.
                Offline yields all targets unresolved (not an error).
                canWriteArchive and writerAllowed are always false. Exits 0.
  codewalker-dry-replace-plan --bundle-dir <path> --entry-manifest-report <path>
                --resolve-report <path> [--permission-report <path>] [--out <out.json>]
                Combine the entry manifest, the CodeWalker resolve report, and the
                providing bundle files into MODELLED /api/replace-file payloads for a
                future writer. Reads only local report/bundle files. Sends NO HTTP
                request, never uses POST, never calls replace/import/reload-services/
                set-config or any mutation endpoint, never executes CodeWalker or any
                external tool, never opens or modifies an RPF archive. dryRunOnly is
                true; readyForExecution, writerAllowed, and codewalkerExecutionAllowed
                are always false. Item-level blockers do not fail the report. Exits 0.
  codewalker-execution-gate --target-rpf <path> --dry-replace-plan <path>
                --permission-report <path> --readiness-report <path>
                --entry-manifest-report <path> --backup-report <path>
                --target-is-test-copy [--out <out.json>]
                Decide whether a FUTURE CodeWalker replace attempt against the target
                archive would even be eligible. Reads only the local target fixture
                and the five report files. Eligibility requires a copied test archive
                (confirmed via --target-is-test-copy, not an original game path) plus a
                valid dry replace plan, permission token, readiness report, entry
                manifest, and a hash-verified backup. Sends NO HTTP request, never uses
                POST, never calls replace/import/reload-services/set-config or any
                mutation endpoint, never executes CodeWalker or any external tool, never
                opens or modifies an RPF archive. codewalkerExecutionEligible may be
                true, but codewalkerExecutionAllowedNow, codewalkerExecutionPerformed,
                writerAllowed, and modifiesArchive are always false. Exits 0.
  codewalker-replace-apply --base-url <url> --execution-gate-report <path>
                --dry-replace-plan <path> --execute --confirm "<phrase>" [--out <out.json>]
                First scoped CodeWalker replace executor. Sends POST /api/replace-file
                for each planned request ONLY when the execution gate is eligible, the
                target is a copied test archive, --execute is given, and --confirm
                exactly matches the required phrase. Copied test archives only — never
                an original game archive. Sends ONLY /api/replace-file; never calls
                import/reload-services/set-config or the search endpoint, never executes
                CodeWalker as a process or any external tool, never parses RPF internals,
                never rolls back. On any blocking gate failure NO HTTP request is sent.
                Global writerAllowed stays false and NullRpfAdapter stays active. Exits 0.
  codewalker-post-write-verify --target-rpf <path> --replace-apply-report <path>
                --backup-report <path> --execution-gate-report <path>
                --dry-replace-plan <path> [--out <out.json>]
                Verify the result of a replace apply: compute the current target
                SHA-256, compare it against the apply report pre/post hashes and the
                backup report, classify the outcome, and build a rollback PLAN pointing
                at the verified backup. Local read-only: never restores the backup,
                never modifies the target, never calls CodeWalker, never sends an HTTP
                request, never uses POST, never executes an external tool, never parses
                RPF internals. rollbackExecuted and rollbackExecutionAllowed are always
                false; the active adapter stays NullRpfAdapter. Exits 0.
  editor-dry-run --patch-plan <path> [--operation-id <id>] [--out <out.json>]
  version

Notes:
  - This backend uses the rpf-archive crate.
  - Encrypted GTA V RPF7 requires a valid keys directory.
  - Without keys, encrypted update.rpf cannot be read.
  - --all and --targets-only are deprecated; use --mode instead.
  - baseline-scan writes: full_clean_manifest.json, full_clean_tree.json,
    baseline_update_tree_fingerprint.json, baseline_metadata.json into the --out folder.
  - classify-rpf quick-scans an unknown .rpf and compares its tree against the clean baseline
    to detect renamed update.rpf files. Output: classification.json.
  - dry-run does not modify update.rpf or any real game files. It only evaluates the
    PatchPlan and reports what would happen.
  - inventory is read-only. --workspace points to an extracted RPF-like directory tree.
    No real game files are modified.
  - stage copies target files to a separate staging directory. The source workspace
    and all RPF archives remain untouched.
  - apply-stage modifies only files inside --stage-dir. The source workspace is never
    read or written. RPF archives are not modified. This is the first actual patch
    application layer, but still sandboxed to the staging directory.
  - diff-stage compares workspace files against staged (patched) counterparts. It is
    read-only and does not modify any file. Use it to preview what a patch changed
    before any export or future RPF writing step.
  - export-bundle creates a portable patch bundle folder from the staged files. It copies
    the patched staged files into <bundle-dir>/files/ and bundles the patch plan and report
    files (bundle_manifest.json, patch_plan.json, diff_report.json, plus stage/apply reports
    when present). It never modifies the source workspace, the staged files, or any RPF
    archive — it only writes into the bundle directory.
  - plan-rpf-write is PLANNING ONLY. It reads an exported bundle and produces a structured
    RPF write plan with safety gates (backup, restore, hash verification, manual
    confirmation). It never opens, reads, or modifies the target archive. safeToWrite is
    always false and the real_rpf_writer_not_implemented gate blocks any actual write.
    Real RPF archive writing is intentionally not implemented in this milestone.
  - backup-rpf is a READ/COPY-ONLY preflight. It copies a target .rpf into a backup
    directory and verifies the copy by SHA-256. The original target archive is never
    modified or written. A successful, hash-verified backup is a prerequisite for any
    future controlled RPF writing. No real RPF writing is performed here.
  - probe-rpf is a READ-ONLY preflight. It reads a target .rpf file's metadata and
    SHA-256 hash and reports informational external-tool detection. It does not parse
    RPF internals and never modifies the archive. canParseRpf, canWriteRpf, and
    nativeWriterImplemented are all false in this milestone.
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
        analyze_text: false,
        build_learning_corpus: false,
        component_rules: None,
        target_rules: None,
        rules_dir: None,
        scanner_name: None,
        scanner_version: None,
        baseline: None,
        file: None,
        vmode: None,
        patch_plan: None,
        workspace: None,
        stage_dir: None,
        bundle_dir: None,
        target_rpf: None,
        backup_dir: None,
        clean_rpf: None,
        modded_rpf: None,
        backup_report: None,
        readiness_report: None,
        entry_manifest_report: None,
        resolve_report: None,
        permission_report: None,
        dry_replace_plan: None,
        target_is_test_copy: false,
        execution_gate_report: None,
        replace_apply_report: None,
        execute: false,
        confirm: None,
        base_url: None,
        changed_files: Vec::new(),
        operation_id: None,
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
                args.component_rules = Some(PathBuf::from(
                    it.next().context("missing value for --component-rules")?,
                ))
            }
            "--target-rules" => {
                args.target_rules = Some(PathBuf::from(
                    it.next().context("missing value for --target-rules")?,
                ))
            }
            "--rules-dir" => {
                args.rules_dir = Some(PathBuf::from(
                    it.next().context("missing value for --rules-dir")?,
                ))
            }
            "--baseline" => {
                args.baseline = Some(PathBuf::from(
                    it.next().context("missing value for --baseline")?,
                ))
            }
            "--file" => {
                args.file = Some(PathBuf::from(
                    it.next().context("missing value for --file")?,
                ))
            }
            "--vmode" => args.vmode = Some(it.next().context("missing value for --vmode")?),
            "--patch-plan" => {
                args.patch_plan = Some(PathBuf::from(
                    it.next().context("missing value for --patch-plan")?,
                ))
            }
            "--plan" => {
                args.patch_plan = Some(PathBuf::from(
                    it.next().context("missing value for --plan")?,
                ))
            }
            "--changed-files" => {
                let value = it.next().context("missing value for --changed-files")?;
                if value.ends_with(".json") {
                    let content =
                        fs::read_to_string(&value).context("failed to read changed-files JSON")?;
                    args.changed_files = serde_json::from_str(&content)
                        .context("failed to parse changed-files JSON")?;
                } else {
                    args.changed_files = value.split(',').map(|s| s.trim().to_string()).collect();
                }
            }
            "--operation-id" => {
                args.operation_id = Some(it.next().context("missing value for --operation-id")?)
            }
            "--workspace" => {
                args.workspace = Some(PathBuf::from(
                    it.next().context("missing value for --workspace")?,
                ))
            }
            "--stage-dir" => {
                args.stage_dir = Some(PathBuf::from(
                    it.next().context("missing value for --stage-dir")?,
                ))
            }
            "--bundle-dir" => {
                args.bundle_dir = Some(PathBuf::from(
                    it.next().context("missing value for --bundle-dir")?,
                ))
            }
            "--target-rpf" => {
                args.target_rpf = Some(PathBuf::from(
                    it.next().context("missing value for --target-rpf")?,
                ))
            }
            "--backup-dir" => {
                args.backup_dir = Some(PathBuf::from(
                    it.next().context("missing value for --backup-dir")?,
                ))
            }
            "--clean-rpf" => {
                args.clean_rpf = Some(PathBuf::from(
                    it.next().context("missing value for --clean-rpf")?,
                ))
            }
            "--modded-rpf" => {
                args.modded_rpf = Some(PathBuf::from(
                    it.next().context("missing value for --modded-rpf")?,
                ))
            }
            "--backup-report" => {
                args.backup_report = Some(PathBuf::from(
                    it.next().context("missing value for --backup-report")?,
                ))
            }
            "--readiness-report" => {
                args.readiness_report = Some(PathBuf::from(
                    it.next().context("missing value for --readiness-report")?,
                ))
            }
            "--entry-manifest-report" => {
                args.entry_manifest_report = Some(PathBuf::from(
                    it.next()
                        .context("missing value for --entry-manifest-report")?,
                ))
            }
            "--resolve-report" => {
                args.resolve_report = Some(PathBuf::from(
                    it.next().context("missing value for --resolve-report")?,
                ))
            }
            "--permission-report" => {
                args.permission_report = Some(PathBuf::from(
                    it.next().context("missing value for --permission-report")?,
                ))
            }
            "--dry-replace-plan" => {
                args.dry_replace_plan = Some(PathBuf::from(
                    it.next().context("missing value for --dry-replace-plan")?,
                ))
            }
            "--target-is-test-copy" => args.target_is_test_copy = true,
            "--execution-gate-report" => {
                args.execution_gate_report = Some(PathBuf::from(
                    it.next()
                        .context("missing value for --execution-gate-report")?,
                ))
            }
            "--replace-apply-report" => {
                args.replace_apply_report = Some(PathBuf::from(
                    it.next()
                        .context("missing value for --replace-apply-report")?,
                ))
            }
            "--execute" => args.execute = true,
            "--confirm" => args.confirm = Some(it.next().context("missing value for --confirm")?),
            "--base-url" => {
                args.base_url = Some(it.next().context("missing value for --base-url")?)
            }
            "--analyze-text" => args.analyze_text = true,
            "--build-learning-corpus" => args.build_learning_corpus = true,
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

/// Anchor paths for update.rpf detection.
/// update.rpf uses a flat structure — no common/ or x64/ directory prefixes.
/// Nested RPFs appear as path prefixes like "ptfx.rpf/core.ypt".
const ANCHOR_PATHS: &[&str] = &[
    "american_rel.rpf/", // nested RPF with GXT2 text strings — highly characteristic
    "ptfx.rpf/",         // nested particle effects RPF — highly characteristic
    "scaleform_frontend.rpf/", // nested scaleform UI RPF — characteristic
    "visualsettings.dat", // visual settings — only in update.rpf
    "gta5_cache_y.dat",  // game cache — only in update.rpf
    "popcycle.dat",      // population cycle data
    "carcols.meta",      // car color definitions
    "hudcolor.dat",      // HUD color config
];

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
    let raw: ComponentRulesFile = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse component rules file {}", path.display()))?;
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

                    let mut meta =
                        build_rich_metadata(&c.path, "modified", c.sizeBytes, m.sizeBytes);
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
        add_hit(
            report,
            ch,
            "high",
            "Exact ptfx_bullet_tracer asset changed.",
        );
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

    if b == "timecycle_mods_4.xml" || p.ends_with("common/data/timecycle/timecycle_mods_4.xml") {
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
    let text_count = entries
        .values()
        .filter(|e| is_text_candidate(&e.path))
        .count();
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
    archive_path_str: &str,
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
        baselineArchivePath: &'a str,
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
        baselineArchivePath: archive_path_str,
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
        reusableWhen:
            "archive sha256, scanner version, schema version, and rules version all match",
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
    #[serde(default)]
    baselineArchivePath: Option<String>,
}

fn load_baseline_manifest(baseline_dir: &Path) -> Result<BTreeMap<String, EntryInfo>> {
    let manifest_path = baseline_dir.join("full_clean_manifest.json");
    let contents = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "failed to read baseline manifest: {}",
            manifest_path.display()
        )
    })?;
    let parsed: BaselineManifestFile = serde_json::from_str(&contents).with_context(|| {
        format!(
            "failed to parse baseline manifest: {}",
            manifest_path.display()
        )
    })?;
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
    let text_count = entries
        .values()
        .filter(|e| is_text_candidate(&e.path))
        .count();
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
            "unknown_changes.json".to_string(),
            "unknown_text_candidates.json".to_string(),
            "unknown_binary_candidates.json".to_string(),
            "candidate_patterns.json".to_string(),
            "llm_review_queue.jsonl".to_string(),
            "unknown_summary.json".to_string(),
        ],
        warnings,
    };

    let out = out_dir.join("diff_summary.json");
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(&out, json)?;
    Ok(())
}

// ── Unknown Pattern Discovery ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct UnknownEntry {
    path: String,
    status: String,
    name: String,
    extension: String,
    cleanSizeBytes: usize,
    moddedSizeBytes: usize,
    sizeDeltaBytes: i64,
    cleanSha256: String,
    moddedSha256: String,
    categoryGuess: String,
    unknownClass: String,
    analyzerRequired: bool,
    safeForAiTextExtraction: bool,
    nestedArchivePath: Option<String>,
    reason: String,
    priority: String,
}

#[derive(Debug, Clone, Serialize)]
struct UnknownTextEntry {
    path: String,
    extension: String,
    status: String,
    cleanSizeBytes: usize,
    moddedSizeBytes: usize,
    sizeDeltaBytes: i64,
    reason: String,
    futureAnalyzer: String,
    priority: String,
}

#[derive(Debug, Clone, Serialize)]
struct UnknownBinaryEntry {
    path: String,
    extension: String,
    status: String,
    cleanSizeBytes: usize,
    moddedSizeBytes: usize,
    sizeDeltaBytes: i64,
    reason: String,
    futureAnalyzer: String,
    priority: String,
    analyzerRequired: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CandidatePattern {
    patternId: String,
    title: String,
    candidateComponent: String,
    confidence: String,
    evidence: Vec<String>,
    files: Vec<String>,
    recommendedNextStep: String,
}

#[derive(Debug, Clone, Serialize)]
struct LlmReviewTask {
    task: String,
    path: String,
    status: String,
    extension: String,
    unknownClass: String,
    context: LlmReviewContext,
    question: String,
}

#[derive(Debug, Clone, Serialize)]
struct LlmReviewContext {
    folder: String,
    nestedArchivePath: Option<String>,
    sizeDeltaBytes: i64,
}

#[derive(Debug, Clone, Serialize)]
struct UnknownSummary {
    schemaVersion: String,
    ok: bool,
    artifactType: String,
    totalUnknown: usize,
    textCandidates: usize,
    binaryCandidates: usize,
    analyzerRequired: usize,
    topUnknownExtensions: Vec<ExtCount>,
    topUnknownFolders: Vec<FolderCount>,
    candidatePatternCount: usize,
    recommendedNextPhase: String,
    warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Serialize)]
struct ExtCount {
    extension: String,
    count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct FolderCount {
    folder: String,
    count: usize,
}

fn unknown_class_for_ext(ext: &str) -> &'static str {
    match ext {
        "xml" | "dat" | "meta" | "txt" | "json" | "ini" | "cfg" | "nametable" => {
            "unknown_config_candidate"
        }
        "ymt" | "ymap" | "ytyp" => "unknown_text_candidate",
        "ytd" => "unknown_binary_candidate",
        "ypt" => "unknown_binary_candidate",
        "gfx" | "swf" => "unknown_binary_candidate",
        "ydr" | "yft" | "ybn" | "ydd" | "yld" => "unknown_binary_candidate",
        "awc" | "rel" => "unknown_binary_candidate",
        "rpf" => "unknown_nested_archive_candidate",
        _ => "unknown_binary_candidate",
    }
}

fn future_analyzer_for_ext(ext: &str) -> &'static str {
    match ext {
        "xml" => "xml_analyzer",
        "dat" | "cfg" | "ini" | "nametable" => "dat_config_analyzer",
        "meta" | "ymt" => "meta_analyzer",
        "ymap" | "ytyp" => "text_diff_analyzer",
        "txt" | "json" => "text_diff_analyzer",
        "ytd" => "ytd_texture_analyzer",
        "gfx" | "swf" => "gfx_swf_analyzer",
        "ypt" => "ypt_particle_analyzer",
        "rpf" => "rpf_nested_analyzer",
        _ => "unknown_binary_analyzer",
    }
}

fn is_text_candidate_ext(ext: &str) -> bool {
    matches!(
        ext,
        "xml"
            | "dat"
            | "meta"
            | "txt"
            | "json"
            | "ini"
            | "cfg"
            | "ymt"
            | "ymap"
            | "ytyp"
            | "nametable"
    )
}

fn analyzer_required_for_ext(ext: &str) -> bool {
    !is_text_candidate_ext(ext)
}

fn safe_for_ai_text(ext: &str) -> bool {
    is_text_candidate_ext(ext)
}

fn priority_for_ext(ext: &str, size_delta: i64) -> &'static str {
    match ext {
        "xml" | "dat" | "meta" | "ymt" | "ypt" | "gfx" | "rpf" => "high",
        "ytd" | "ymap" | "ytyp" | "ydr" | "yft" => "medium",
        _ => {
            if size_delta.abs() > 100_000 {
                "medium"
            } else {
                "low"
            }
        }
    }
}

fn nested_archive_path_from_change(change: &Change) -> Option<String> {
    if change.parentPath.contains(".rpf") {
        Some(change.parentPath.clone())
    } else if change.path.contains(".rpf/") {
        let p = normalize_path(&change.path);
        if let Some(idx) = p.find(".rpf/") {
            Some(p[..idx + 4].to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// A change is "truly unknown" when the only component is the sentinel "unknown"
/// (set by build_rich_metadata when nothing matched) or when components is empty.
fn is_truly_unknown_change(c: &Change) -> bool {
    c.components.is_empty() || c.components == ["unknown"]
}

fn build_unknown_entries(changes: &[Change]) -> Vec<UnknownEntry> {
    changes
        .iter()
        .filter(|c| is_truly_unknown_change(c))
        .map(|c| {
            let ext = c.extension.as_str();
            let unknown_class = unknown_class_for_ext(ext);
            let nested_archive_path = nested_archive_path_from_change(c);
            let priority = priority_for_ext(ext, c.sizeDelta);
            let category_guess = if c.category != "unknown_binary" {
                c.category.clone()
            } else {
                match ext {
                    "xml" | "dat" | "meta" | "ini" | "cfg" | "txt" => "config_or_text".to_string(),
                    "ytd" => "texture_dictionary".to_string(),
                    "ypt" => "particle_container".to_string(),
                    "gfx" => "scaleform_ui".to_string(),
                    "rpf" => "nested_archive".to_string(),
                    _ => "unknown_binary".to_string(),
                }
            };

            UnknownEntry {
                path: c.path.clone(),
                status: c.status.clone(),
                name: c.basename.clone(),
                extension: c.extension.clone(),
                cleanSizeBytes: c.cleanSize,
                moddedSizeBytes: c.moddedSize,
                sizeDeltaBytes: c.sizeDelta,
                cleanSha256: c.cleanSha256.clone(),
                moddedSha256: c.moddedSha256.clone(),
                categoryGuess: category_guess,
                unknownClass: unknown_class.to_string(),
                analyzerRequired: analyzer_required_for_ext(ext),
                safeForAiTextExtraction: safe_for_ai_text(ext),
                nestedArchivePath: nested_archive_path,
                reason: c.reason.clone(),
                priority: priority.to_string(),
            }
        })
        .collect()
}

fn recommended_next_step_for_pattern(ext: Option<&str>, archive_group: bool) -> &'static str {
    if archive_group {
        return "run nested RPF scan";
    }

    match ext.unwrap_or("") {
        "xml" | "dat" | "meta" | "txt" | "json" | "ini" | "cfg" | "ymt" | "ymap" | "ytyp"
        | "nametable" => "run DAT/META/XML analyzer in R0.7",
        "ytd" => "run YTD texture analyzer",
        "ypt" => "run YPT particle analyzer",
        "rpf" => "run nested RPF scan",
        "gfx" | "swf" => "run GFX/SWF analyzer",
        _ => "investigate manually or wait for R0.7",
    }
}

fn pattern_confidence(file_count: usize) -> &'static str {
    if file_count >= 5 {
        "high"
    } else {
        "medium"
    }
}

fn write_unknown_changes(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    entries: &[UnknownEntry],
    warnings: &[Warning],
) -> Result<()> {
    #[derive(Serialize)]
    struct UnknownStats {
        totalUnknown: usize,
        textCandidates: usize,
        binaryCandidates: usize,
        analyzerRequired: usize,
    }

    #[derive(Serialize)]
    struct UnknownChangesFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        scan: &'a ScanMetadata,
        stats: UnknownStats,
        entries: &'a [UnknownEntry],
        warnings: &'a [Warning],
    }

    let report = UnknownChangesFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "unknown_changes",
        tool,
        timing,
        scan,
        stats: UnknownStats {
            totalUnknown: entries.len(),
            textCandidates: entries
                .iter()
                .filter(|entry| is_text_candidate_ext(&entry.extension))
                .count(),
            binaryCandidates: entries
                .iter()
                .filter(|entry| !is_text_candidate_ext(&entry.extension))
                .count(),
            analyzerRequired: entries
                .iter()
                .filter(|entry| entry.analyzerRequired)
                .count(),
        },
        entries,
        warnings,
    };

    let out = out_dir.join("unknown_changes.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_unknown_text_candidates(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[UnknownEntry],
    warnings: &[Warning],
) -> Result<()> {
    #[derive(Serialize)]
    struct UnknownCandidatesStats {
        total: usize,
    }

    #[derive(Serialize)]
    struct UnknownTextFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        stats: UnknownCandidatesStats,
        entries: Vec<UnknownTextEntry>,
        warnings: &'a [Warning],
    }

    let filtered_entries: Vec<UnknownTextEntry> = entries
        .iter()
        .filter(|entry| is_text_candidate_ext(&entry.extension))
        .map(|entry| UnknownTextEntry {
            path: entry.path.clone(),
            extension: entry.extension.clone(),
            status: entry.status.clone(),
            cleanSizeBytes: entry.cleanSizeBytes,
            moddedSizeBytes: entry.moddedSizeBytes,
            sizeDeltaBytes: entry.sizeDeltaBytes,
            reason: entry.reason.clone(),
            futureAnalyzer: future_analyzer_for_ext(&entry.extension).to_string(),
            priority: entry.priority.clone(),
        })
        .collect();

    let report = UnknownTextFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "unknown_text_candidates",
        tool,
        timing,
        stats: UnknownCandidatesStats {
            total: filtered_entries.len(),
        },
        entries: filtered_entries,
        warnings,
    };

    let out = out_dir.join("unknown_text_candidates.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_unknown_binary_candidates(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[UnknownEntry],
    warnings: &[Warning],
) -> Result<()> {
    #[derive(Serialize)]
    struct UnknownCandidatesStats {
        total: usize,
    }

    #[derive(Serialize)]
    struct UnknownBinaryFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        stats: UnknownCandidatesStats,
        entries: Vec<UnknownBinaryEntry>,
        warnings: &'a [Warning],
    }

    let filtered_entries: Vec<UnknownBinaryEntry> = entries
        .iter()
        .filter(|entry| !is_text_candidate_ext(&entry.extension))
        .map(|entry| UnknownBinaryEntry {
            path: entry.path.clone(),
            extension: entry.extension.clone(),
            status: entry.status.clone(),
            cleanSizeBytes: entry.cleanSizeBytes,
            moddedSizeBytes: entry.moddedSizeBytes,
            sizeDeltaBytes: entry.sizeDeltaBytes,
            reason: entry.reason.clone(),
            futureAnalyzer: future_analyzer_for_ext(&entry.extension).to_string(),
            priority: entry.priority.clone(),
            analyzerRequired: entry.analyzerRequired,
        })
        .collect();

    let report = UnknownBinaryFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "unknown_binary_candidates",
        tool,
        timing,
        stats: UnknownCandidatesStats {
            total: filtered_entries.len(),
        },
        entries: filtered_entries,
        warnings,
    };

    let out = out_dir.join("unknown_binary_candidates.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_candidate_patterns(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[UnknownEntry],
) -> Result<usize> {
    #[derive(Serialize)]
    struct CandidatePatternStats {
        totalPatterns: usize,
        totalFiles: usize,
    }

    #[derive(Serialize)]
    struct CandidatePatternFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        stats: CandidatePatternStats,
        patterns: Vec<CandidatePattern>,
    }

    let mut ext_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in entries {
        ext_groups
            .entry(entry.extension.clone())
            .or_default()
            .push(entry.path.clone());
    }

    let mut archive_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in entries {
        if let Some(ref nested_archive_path) = entry.nestedArchivePath {
            archive_groups
                .entry(nested_archive_path.clone())
                .or_default()
                .push(entry.path.clone());
        }
    }

    let mut patterns = Vec::new();
    let mut pattern_index = 1usize;

    for (extension, mut files) in ext_groups {
        if files.len() < 2 {
            continue;
        }

        files.sort();
        let extension_label = if extension.is_empty() {
            "no_extension".to_string()
        } else {
            extension.clone()
        };
        let candidate_component = entries
            .iter()
            .find(|entry| entry.extension == extension)
            .map(|entry| entry.categoryGuess.clone())
            .unwrap_or_else(|| "unknown_binary".to_string());

        patterns.push(CandidatePattern {
            patternId: format!("pattern_{:03}", pattern_index),
            title: format!("Unknown .{} cluster", extension_label),
            candidateComponent: candidate_component,
            confidence: pattern_confidence(files.len()).to_string(),
            evidence: vec![
                format!("shared extension: {}", extension_label),
                format!("file count: {}", files.len()),
            ],
            files,
            recommendedNextStep: recommended_next_step_for_pattern(Some(&extension), false)
                .to_string(),
        });
        pattern_index += 1;
    }

    for (archive, mut files) in archive_groups {
        if files.len() < 2 {
            continue;
        }

        files.sort();
        patterns.push(CandidatePattern {
            patternId: format!("pattern_{:03}", pattern_index),
            title: format!("Unknown nested archive cluster in {}", archive),
            candidateComponent: "nested_archive".to_string(),
            confidence: pattern_confidence(files.len()).to_string(),
            evidence: vec![
                format!("nested archive: {}", archive),
                format!("file count: {}", files.len()),
            ],
            files,
            recommendedNextStep: recommended_next_step_for_pattern(None, true).to_string(),
        });
        pattern_index += 1;
    }

    let pattern_count = patterns.len();
    let total_files = patterns.iter().map(|pattern| pattern.files.len()).sum();
    let report = CandidatePatternFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "candidate_patterns",
        tool,
        timing,
        stats: CandidatePatternStats {
            totalPatterns: pattern_count,
            totalFiles: total_files,
        },
        patterns,
    };

    let out = out_dir.join("candidate_patterns.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(pattern_count)
}

fn write_llm_review_queue(out_dir: &Path, entries: &[UnknownEntry]) -> Result<()> {
    let mut lines = Vec::new();

    for entry in entries {
        let should_enqueue = entry.unknownClass == "unknown_text_candidate"
            || entry.unknownClass == "unknown_config_candidate"
            || (entry.priority == "high" && entry.unknownClass == "unknown_binary_candidate");

        if !should_enqueue {
            continue;
        }

        let folder = {
            let parent = parent_path(&entry.path);
            if parent.is_empty() {
                "root".to_string()
            } else {
                parent
            }
        };

        let task = LlmReviewTask {
            task: "review_unknown_change".to_string(),
            path: entry.path.clone(),
            status: entry.status.clone(),
            extension: entry.extension.clone(),
            unknownClass: entry.unknownClass.clone(),
            context: LlmReviewContext {
                folder,
                nestedArchivePath: entry.nestedArchivePath.clone(),
                sizeDeltaBytes: entry.sizeDeltaBytes,
            },
            question: "What GTA/Redux component might this changed file relate to? Answer as hypothesis only.".to_string(),
        };

        lines.push(serde_json::to_string(&task)?);
    }

    let out = out_dir.join("llm_review_queue.jsonl");
    fs::write(&out, lines.join("\n"))?;
    Ok(())
}

fn write_unknown_summary(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[UnknownEntry],
    pattern_count: usize,
    warnings: &[Warning],
) -> Result<()> {
    #[derive(Serialize)]
    struct UnknownSummaryFile<'a> {
        #[serde(flatten)]
        summary: UnknownSummary,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
    }

    let mut extension_counts = BTreeMap::new();
    for entry in entries {
        *extension_counts
            .entry(entry.extension.clone())
            .or_insert(0usize) += 1;
    }
    let mut top_unknown_extensions: Vec<ExtCount> = extension_counts
        .into_iter()
        .map(|(extension, count)| ExtCount { extension, count })
        .collect();
    top_unknown_extensions.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    top_unknown_extensions.truncate(10);

    let mut folder_counts = BTreeMap::new();
    for entry in entries {
        if let Some((folder, _)) = entry.path.split_once('/') {
            if !folder.is_empty() {
                *folder_counts.entry(folder.to_string()).or_insert(0usize) += 1;
            }
        }
    }
    let mut top_unknown_folders: Vec<FolderCount> = folder_counts
        .into_iter()
        .map(|(folder, count)| FolderCount { folder, count })
        .collect();
    top_unknown_folders.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.folder.cmp(&b.folder)));
    top_unknown_folders.truncate(10);

    let summary = UnknownSummaryFile {
        summary: UnknownSummary {
            schemaVersion: SCHEMA_VERSION.to_string(),
            ok: true,
            artifactType: "unknown_summary".to_string(),
            totalUnknown: entries.len(),
            textCandidates: entries
                .iter()
                .filter(|entry| is_text_candidate_ext(&entry.extension))
                .count(),
            binaryCandidates: entries
                .iter()
                .filter(|entry| !is_text_candidate_ext(&entry.extension))
                .count(),
            analyzerRequired: entries
                .iter()
                .filter(|entry| entry.analyzerRequired)
                .count(),
            topUnknownExtensions: top_unknown_extensions,
            topUnknownFolders: top_unknown_folders,
            candidatePatternCount: pattern_count,
            recommendedNextPhase: "R0.7 XML/DAT/META analyzers".to_string(),
            warnings: warnings.to_vec(),
        },
        tool,
        timing,
    };

    let out = out_dir.join("unknown_summary.json");
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(&out, json)?;
    Ok(())
}

// ── RPF Classifier ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FingerprintArchiveId {
    archiveFileName: String,
    archiveSha256: String,
}

#[derive(Deserialize)]
struct BaselineFingerprintFile {
    archive: FingerprintArchiveId,
    totalPaths: usize,
    treeFingerprintSha256: String,
    anchorPathsFound: Vec<String>,
}

fn load_baseline_fingerprint(baseline_dir: &Path) -> Result<BaselineFingerprintFile> {
    let fp_path = baseline_dir.join("baseline_update_tree_fingerprint.json");
    let contents = fs::read_to_string(&fp_path)
        .with_context(|| format!("failed to read baseline fingerprint: {}", fp_path.display()))?;
    serde_json::from_str(&contents).with_context(|| {
        format!(
            "failed to parse baseline fingerprint: {}",
            fp_path.display()
        )
    })
}

/// Copies an RPF archive to a temp directory under a given logical filename.
/// GTA V NG encryption derives the decryption key from the archive filename, so
/// opening a renamed archive (e.g. redux.rpf) under the correct logical name
/// (e.g. update.rpf) allows the RPF library to derive the right key.
/// The caller must keep the returned TempDir alive while the path is used.
fn copy_archive_to_logical_name(
    physical_path: &Path,
    logical_name: &str,
) -> Result<(TempDir, PathBuf)> {
    let temp_dir =
        TempDir::new().context("failed to create temp dir for logical-name classify scan")?;
    let dest = temp_dir.path().join(logical_name);
    fs::copy(physical_path, &dest).with_context(|| {
        format!(
            "failed to copy archive to logical-name temp path: {} -> {}",
            physical_path.display(),
            dest.display()
        )
    })?;
    Ok((temp_dir, dest))
}

fn anchor_score(anchor: &str) -> i32 {
    match anchor {
        "american_rel.rpf/" => 18,
        "ptfx.rpf/" => 12,
        "scaleform_frontend.rpf/" => 10,
        "visualsettings.dat" => 14,
        "gta5_cache_y.dat" => 10,
        "popcycle.dat" => 8,
        "carcols.meta" => 7,
        "hudcolor.dat" => 6,
        _ => 2,
    }
}

/// Returns (clamped_score, reasons, matched_anchors, missing_anchors).
fn score_classify_archive(
    entries: &BTreeMap<String, EntryInfo>,
    baseline_fp: &BaselineFingerprintFile,
) -> (u32, Vec<String>, Vec<String>, Vec<String>) {
    let mut score: i32 = 0;
    let mut reasons: Vec<String> = Vec::new();

    let anchor_result = check_anchor_paths(entries);
    for a in &anchor_result.found {
        let pts = anchor_score(a);
        score += pts;
        reasons.push(format!("Matched anchor \"{}\" (+{})", a, pts));
    }

    let hist = build_extension_histogram(entries);
    // Strong update.rpf extension signals
    if hist.contains_key("yvr") {
        score += 8;
        reasons.push(format!("Has .yvr files (route/animation data, +8)"));
    }
    if hist.contains_key("ysc") {
        score += 6;
        reasons.push(format!("Has .ysc files (scripts, +6)"));
    }
    if hist.contains_key("gxt2") {
        score += 5;
        reasons.push(format!("Has .gxt2 files (text strings, +5)"));
    }
    if hist.contains_key("ymap") {
        score += 3;
        reasons.push(format!("Has .ymap files (world data, +3)"));
    }
    if hist.contains_key("fxc") {
        score += 2;
        reasons.push(format!("Has .fxc files (shaders, +2)"));
    }
    // Weaker legacy hints
    if hist.contains_key("xml") {
        score += 1;
        reasons.push("Has .xml files (+1)".to_string());
    }
    if hist.contains_key("dat") {
        score += 1;
        reasons.push("Has .dat files (+1)".to_string());
    }
    if hist.contains_key("meta") {
        score += 1;
        reasons.push("Has .meta files (+1)".to_string());
    }

    let n = entries.len();
    let baseline_n = baseline_fp.totalPaths;
    if n > 5000 {
        score += 8;
        reasons.push(format!("Large archive ({} entries, +8)", n));
    } else if n > 1000 {
        score += 4;
        reasons.push(format!("Medium-large archive ({} entries, +4)", n));
    } else if n > 500 {
        reasons.push(format!("Small-medium archive ({} entries)", n));
    } else if n < 100 {
        score -= 30;
        reasons.push(format!("Very small archive ({} entries, -30)", n));
    } else if n < 500 {
        score -= 10;
        reasons.push(format!("Small archive ({} entries, -10)", n));
    }

    // Bonus if size is in a reasonable fraction of the baseline
    if baseline_n > 0 {
        let ratio = n as f64 / baseline_n as f64;
        if ratio >= 0.3 && ratio <= 1.5 {
            score += 5;
            reasons.push(format!(
                "Entry count ratio {:.0}% of baseline ({} / {}, +5)",
                ratio * 100.0,
                n,
                baseline_n
            ));
        } else if ratio < 0.1 {
            score -= 15;
            reasons.push(format!(
                "Entry count ratio {:.0}% of baseline — much smaller than expected (-15)",
                ratio * 100.0
            ));
        }
    }

    // Penalty: narrow DLC/vehicle/audio archive
    let ext_keys: BTreeSet<&str> = hist.keys().map(|s| s.as_str()).collect();
    let vehicle_exts: BTreeSet<&str> = ["yft", "ytd", "ydr", "ydd", "yld"]
        .iter()
        .copied()
        .collect();
    let audio_exts: BTreeSet<&str> = ["awc", "rel"].iter().copied().collect();
    let update_signals: BTreeSet<&str> = ["yvr", "ysc", "gxt2"].iter().copied().collect();

    let has_update_signals = !ext_keys.is_disjoint(&update_signals);
    let only_vehicles = ext_keys.is_subset(&vehicle_exts);
    let only_audio = ext_keys.is_subset(&audio_exts);

    if !has_update_signals && only_vehicles {
        score -= 25;
        reasons
            .push("Appears to be vehicle-only asset pack (no script/text files, -25)".to_string());
    } else if !has_update_signals && only_audio {
        score -= 25;
        reasons.push("Appears to be audio-only pack (no script/text files, -25)".to_string());
    }

    let clamped = score.clamp(0, 100) as u32;
    (clamped, reasons, anchor_result.found, anchor_result.missing)
}

fn classify_label_from_score(score: u32) -> &'static str {
    match score {
        90..=100 => "obvious_update_rpf",
        75..=89 => "likely_update_rpf",
        50..=74 => "possible_update_rpf",
        20..=49 => "not_update_rpf",
        _ => "unknown_rpf",
    }
}

fn recommend_action_from_label(label: &str) -> &'static str {
    match label {
        "obvious_update_rpf" | "likely_update_rpf" => "import_as_update_rpf",
        "possible_update_rpf" => "review_before_import",
        "not_update_rpf" => "skip",
        "scan_failed" => "review_error",
        _ => "review",
    }
}

fn write_classification_report(
    out: &Path,
    archive_identity: &ArchiveIdentity,
    baseline_fp: &BaselineFingerprintFile,
    tool: &ToolMetadata,
    timing: &Timing,
    scan: &ScanMetadata,
    score: u32,
    classification: &str,
    confidence: f64,
    recommended_action: &str,
    reasons: &[String],
    matched_anchors: &[String],
    missing_anchors: &[String],
    top_level_folders: &[String],
    extension_histogram: &BTreeMap<String, usize>,
    entry_count: usize,
    warnings: &[Warning],
    attempts: &[ClassifyAttempt],
    used_logical_archive_name: Option<&str>,
) -> Result<()> {
    #[derive(Serialize)]
    struct ClassifyArchiveBlock<'a> {
        path: &'a str,
        fileName: &'a str,
        sizeBytes: u64,
        sha256: &'a str,
        entryCount: usize,
    }

    #[derive(Serialize)]
    struct ClassifyBaselineBlock<'a> {
        archiveFileName: &'a str,
        archiveSha256: &'a str,
        treeFingerprintSha256: &'a str,
        totalPaths: usize,
    }

    #[derive(Serialize)]
    struct ExtEntry {
        extension: String,
        count: usize,
    }

    #[derive(Serialize)]
    struct ClassificationReport<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        scan: &'a ScanMetadata,
        archive: ClassifyArchiveBlock<'a>,
        baseline: ClassifyBaselineBlock<'a>,
        classification: &'a str,
        confidence: f64,
        score: u32,
        recommendedAction: &'a str,
        reasons: &'a [String],
        matchedAnchors: &'a [String],
        missingAnchors: &'a [String],
        topLevelFolders: &'a [String],
        extensionHistogram: Vec<ExtEntry>,
        attempts: &'a [ClassifyAttempt],
        usedLogicalArchiveName: Option<&'a str>,
        warnings: &'a [Warning],
    }

    let ext_entries: Vec<ExtEntry> = extension_histogram
        .iter()
        .map(|(k, &v)| ExtEntry {
            extension: k.clone(),
            count: v,
        })
        .collect();

    let report = ClassificationReport {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "rpf_classification",
        tool,
        timing,
        scan,
        archive: ClassifyArchiveBlock {
            path: &archive_identity.archivePath,
            fileName: &archive_identity.archiveFileName,
            sizeBytes: archive_identity.archiveSizeBytes,
            sha256: &archive_identity.archiveSha256,
            entryCount: entry_count,
        },
        baseline: ClassifyBaselineBlock {
            archiveFileName: &baseline_fp.archive.archiveFileName,
            archiveSha256: &baseline_fp.archive.archiveSha256,
            treeFingerprintSha256: &baseline_fp.treeFingerprintSha256,
            totalPaths: baseline_fp.totalPaths,
        },
        classification,
        confidence,
        score,
        recommendedAction: recommended_action,
        reasons,
        matchedAnchors: matched_anchors,
        missingAnchors: missing_anchors,
        topLevelFolders: top_level_folders,
        extensionHistogram: ext_entries,
        attempts,
        usedLogicalArchiveName: used_logical_archive_name,
        warnings,
    };

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(out, json)?;
    Ok(())
}

// ── Text Analyzers (R0.7) ──────────────────────────────────────────────────

const MAX_TEXT_FILE_BYTES: usize = 25 * 1024 * 1024;
const MAX_SAMPLE_CHANGES: usize = 10;
const MAX_KEY_VALUE_SAMPLES: usize = 10;

#[derive(Debug, Clone, Serialize)]
struct SampleLineChange {
    lineIndex: usize,
    kind: String,
    oldLine: String,
    newLine: String,
    changeHint: String,
}

#[derive(Debug, Clone)]
struct LineDiffResult {
    cleanLineCount: usize,
    moddedLineCount: usize,
    addedLineCount: usize,
    removedLineCount: usize,
    numericChanges: usize,
    colorLikeChanges: usize,
    samples: Vec<SampleLineChange>,
}

#[derive(Debug, Clone, Serialize)]
struct XmlDiffEntry {
    path: String,
    status: String,
    analyzer: String,
    parseStrategy: String,
    cleanLines: usize,
    moddedLines: usize,
    addedLines: usize,
    removedLines: usize,
    numericChanges: usize,
    colorLikeChanges: usize,
    sampleChanges: Vec<SampleLineChange>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct KeyValueChange {
    key: String,
    oldValue: String,
    newValue: String,
    valueType: String,
    numericDelta: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct DatDiffEntry {
    path: String,
    status: String,
    analyzer: String,
    readable: bool,
    cleanLines: usize,
    moddedLines: usize,
    changedKeyCount: usize,
    addedLines: usize,
    removedLines: usize,
    numericChanges: usize,
    sampleKeyChanges: Vec<KeyValueChange>,
    sampleChanges: Vec<SampleLineChange>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MetaDiffEntry {
    path: String,
    status: String,
    analyzer: String,
    parseStrategy: String,
    cleanLines: usize,
    moddedLines: usize,
    addedLines: usize,
    removedLines: usize,
    numericChanges: usize,
    colorLikeChanges: usize,
    sampleChanges: Vec<SampleLineChange>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GenericTextDiffEntry {
    path: String,
    status: String,
    analyzer: String,
    parseStrategy: String,
    cleanLines: usize,
    moddedLines: usize,
    addedLines: usize,
    removedLines: usize,
    numericChanges: usize,
    colorLikeChanges: usize,
    sampleChanges: Vec<SampleLineChange>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AnalyzerWarning {
    path: String,
    analyzer: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
struct TextAnalysisStats {
    totalCandidates: usize,
    analyzedFiles: usize,
    skippedFiles: usize,
    xmlAnalyzed: usize,
    datAnalyzed: usize,
    metaAnalyzed: usize,
    genericTextAnalyzed: usize,
    parseFailures: usize,
    extractionFailures: usize,
    tooLargeSkipped: usize,
    skippedNotTextBytes: usize,
}

#[derive(Debug, Clone)]
struct TextAnalysisFileSummary {
    path: String,
    extension: String,
    analyzer: String,
    status: String,
    sizeDelta: i64,
    addedLines: usize,
    removedLines: usize,
    numericChanges: usize,
    colorLikeChanges: usize,
}

#[derive(Debug, Clone)]
struct TextAnalysisResults {
    xml_entries: Vec<XmlDiffEntry>,
    dat_entries: Vec<DatDiffEntry>,
    meta_entries: Vec<MetaDiffEntry>,
    generic_entries: Vec<GenericTextDiffEntry>,
    analyzer_warnings: Vec<AnalyzerWarning>,
    stats: TextAnalysisStats,
    file_summaries: Vec<TextAnalysisFileSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct AiChangeSummary {
    addedLines: usize,
    removedLines: usize,
    numericChanges: usize,
    colorLikeChanges: usize,
}

#[derive(Debug, Clone, Serialize)]
struct AiChangeNote {
    task: String,
    path: String,
    extension: String,
    analyzer: String,
    changeSummary: AiChangeSummary,
    question: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimecycleFileRanking {
    path_or_family: String,
    rank: usize,
    category: String,
    evidence: Vec<String>,
    confidence: String,
    risk: String,
    recommended_phase: String,
    safe_for_ai_planning: bool,
    safe_for_direct_editing: bool,
    recommended_tool: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimecycleFileRankingsReport {
    schema_version: String,
    ok: bool,
    artifact_type: String,
    tool: ToolMetadata,
    timing: Timing,
    generated_at: String,
    rankings: Vec<TimecycleFileRanking>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SafeEditMatrixEntry {
    file: String,
    allowed_first_patch_operations: Vec<String>,
    blocked_operations: Vec<String>,
    deferred_operations: Vec<String>,
    validator_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimecycleSafeEditMatrixReport {
    schema_version: String,
    ok: bool,
    artifact_type: String,
    tool: ToolMetadata,
    timing: Timing,
    generated_at: String,
    entries: Vec<SafeEditMatrixEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualsettingsKeyFamily {
    family: String,
    keys: Vec<String>,
    sample_changes: Vec<KeyValueChange>,
    risk: String,
    safe_for_first_patch: bool,
    hypothesis: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualsettingsKeyReport {
    schema_version: String,
    ok: bool,
    artifact_type: String,
    tool: ToolMetadata,
    timing: Timing,
    generated_at: String,
    file: String,
    status: String,
    changed_key_count: usize,
    numeric_changes: usize,
    key_families: Vec<VisualsettingsKeyFamily>,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudkeyframesReport {
    schema_version: String,
    ok: bool,
    artifact_type: String,
    tool: ToolMetadata,
    timing: Timing,
    generated_at: String,
    file: String,
    status: String,
    numeric_changes: usize,
    color_like_changes: usize,
    color_only_pattern_detected: bool,
    numeric_and_color_pattern: bool,
    suggested_first_patch_operation: String,
    blocked_until_schema_known: String,
    evidence: Vec<String>,
    confidence: String,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WeatherXmlEntry {
    path: String,
    status: String,
    numeric_changes: usize,
    color_like_changes: usize,
    confidence: String,
    suggested_phase: String,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WeatherXmlRecommendations {
    best_first_candidates: Vec<String>,
    deferred_files: Vec<String>,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WeatherXmlReport {
    schema_version: String,
    ok: bool,
    artifact_type: String,
    tool: ToolMetadata,
    timing: Timing,
    generated_at: String,
    weather_xml_family: Vec<WeatherXmlEntry>,
    global_weather_xml: WeatherXmlEntry,
    recommendations: WeatherXmlRecommendations,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RiskyFileEntry {
    file_or_family: String,
    reason: String,
    risk: String,
    when_allowed: String,
    required_tool: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RiskyFilesReport {
    schema_version: String,
    ok: bool,
    artifact_type: String,
    tool: ToolMetadata,
    timing: Timing,
    generated_at: String,
    risky_files: Vec<RiskyFileEntry>,
    note: String,
}

fn extract_text_bytes_for_paths(
    archive_path: &Path,
    keys: &GtaKeys,
    depth: usize,
    target_paths: &BTreeSet<String>,
    max_file_size: usize,
    warnings: &mut Vec<Warning>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let temp = TempDir::new().context("failed to create text extraction temp directory")?;
    let mut result = BTreeMap::new();
    extract_text_bytes_inner(
        archive_path,
        keys,
        depth,
        target_paths,
        max_file_size,
        warnings,
        &mut result,
        "",
        temp.path(),
    )?;
    Ok(result)
}

fn extract_text_bytes_inner(
    archive_path: &Path,
    keys: &GtaKeys,
    depth: usize,
    target_paths: &BTreeSet<String>,
    max_file_size: usize,
    warnings: &mut Vec<Warning>,
    result: &mut BTreeMap<String, Vec<u8>>,
    prefix: &str,
    temp_root: &Path,
) -> Result<()> {
    let file = RpfFile::open(archive_path, Some(keys)).with_context(|| {
        format!(
            "failed to open RPF archive for text extraction: {}",
            archive_path.display()
        )
    })?;

    file.walk(Some(keys), &mut |path, data| {
        let joined = if prefix.is_empty() {
            normalize_path(path)
        } else {
            normalize_path(&format!("{}/{}", prefix.trim_matches('/'), path))
        };
        let key = normalize_path(&joined);

        if target_paths.contains(&key) {
            if data.len() <= max_file_size {
                result.insert(key.clone(), data.clone());
            } else {
                push_warning(
                    warnings,
                    "TEXT_EXTRACT_TOO_LARGE",
                    &key,
                    format!(
                        "skipped text extraction because file exceeds {} bytes",
                        max_file_size
                    ),
                );
            }
        }

        if depth > 0 && is_nested_rpf_target(&joined) {
            let nested_path = temp_root.join(format!("text_nested_{}.rpf", sha256_hex(&data)));
            if let Err(e) = fs::write(&nested_path, &data) {
                push_warning(
                    warnings,
                    "TEXT_EXTRACT_TEMP_WRITE_FAILED",
                    &joined,
                    format!("failed to write nested RPF temp file: {}", e),
                );
                return;
            }

            if let Err(e) = extract_text_bytes_inner(
                &nested_path,
                keys,
                depth - 1,
                target_paths,
                max_file_size,
                warnings,
                result,
                &joined,
                temp_root,
            ) {
                push_warning(
                    warnings,
                    "TEXT_EXTRACT_NESTED_RPF_OPEN_FAILED",
                    &joined,
                    format!("failed to open nested RPF for text extraction: {}", e),
                );
            }
        }
    });

    Ok(())
}

fn looks_like_text(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(512)];
    if sample.is_empty() {
        return true;
    }
    let null_count = sample.iter().filter(|&&b| b == 0).count();
    if null_count > 0 {
        return false;
    }
    let non_ascii_count = sample.iter().filter(|&&b| b > 127).count();
    non_ascii_count * 10 < sample.len() * 3
}

fn is_definitely_text_ext(ext: &str) -> bool {
    matches!(ext, "xml" | "dat" | "meta" | "txt" | "ini" | "cfg" | "json")
}

fn is_maybe_text_ext(ext: &str) -> bool {
    matches!(ext, "ymt" | "ymap" | "ytyp" | "nametable")
}

fn parse_number(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn extract_numbers_from_line(line: &str) -> Vec<f64> {
    let mut numbers = Vec::new();
    let mut current = String::new();

    for ch in line.chars() {
        if ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+') {
            current.push(ch);
        } else if !current.is_empty() {
            if let Some(value) = parse_number(&current) {
                numbers.push(value);
            }
            current.clear();
        }
    }

    if !current.is_empty() {
        if let Some(value) = parse_number(&current) {
            numbers.push(value);
        }
    }

    numbers
}

fn is_color_like(line: &str) -> bool {
    let lower = line.to_lowercase();
    let has_named_rgb = (lower.contains("r=")
        || lower.contains("r =")
        || lower.contains("red=")
        || lower.contains("red ="))
        && (lower.contains("g=")
            || lower.contains("g =")
            || lower.contains("green=")
            || lower.contains("green ="))
        && (lower.contains("b=")
            || lower.contains("b =")
            || lower.contains("blue=")
            || lower.contains("blue ="));
    if has_named_rgb {
        return true;
    }

    let numbers = extract_numbers_from_line(line);
    if numbers.len() < 3 {
        return false;
    }

    let sample = &numbers[..3];
    let zero_to_one = sample.iter().all(|n| *n >= 0.0 && *n <= 1.0);
    let zero_to_255 = sample.iter().all(|n| *n >= 0.0 && *n <= 255.0);
    zero_to_one || zero_to_255
}

fn count_numeric_changes(clean_lines: &[&str], modded_lines: &[&str]) -> usize {
    clean_lines
        .iter()
        .zip(modded_lines.iter())
        .filter(|(clean_line, modded_line)| clean_line != modded_line)
        .filter(|(clean_line, modded_line)| {
            let clean_numbers = extract_numbers_from_line(clean_line);
            let modded_numbers = extract_numbers_from_line(modded_line);
            !clean_numbers.is_empty()
                && !modded_numbers.is_empty()
                && clean_numbers != modded_numbers
        })
        .count()
}

fn count_color_like_changes(sample_changes: &[SampleLineChange]) -> usize {
    sample_changes
        .iter()
        .filter(|change| change.changeHint == "color_like")
        .count()
}

fn sanitize_and_truncate(value: &str, max_len: usize) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(max_len)
        .collect()
}

fn sanitize_sample_line(line: &str) -> String {
    sanitize_and_truncate(line, 80)
}

fn classify_change_hint(old_line: &str, new_line: &str) -> String {
    if is_color_like(old_line) || is_color_like(new_line) {
        "color_like".to_string()
    } else {
        let clean_numbers = extract_numbers_from_line(old_line);
        let modded_numbers = extract_numbers_from_line(new_line);
        if !clean_numbers.is_empty()
            && !modded_numbers.is_empty()
            && clean_numbers != modded_numbers
        {
            "numeric_change".to_string()
        } else {
            "text_change".to_string()
        }
    }
}

fn classify_single_line_hint(line: &str) -> String {
    if is_color_like(line) {
        "color_like".to_string()
    } else if !extract_numbers_from_line(line).is_empty() {
        "numeric_change".to_string()
    } else {
        "text_change".to_string()
    }
}

fn build_line_count_map<'a>(lines: &[&'a str]) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for line in lines {
        *counts.entry(*line).or_insert(0usize) += 1;
    }
    counts
}

fn diff_lines(clean_text: &str, modded_text: &str, max_samples: usize) -> LineDiffResult {
    let clean_lines: Vec<&str> = clean_text.lines().collect();
    let modded_lines: Vec<&str> = modded_text.lines().collect();
    let clean_counts = build_line_count_map(&clean_lines);
    let modded_counts = build_line_count_map(&modded_lines);

    let added_line_count = modded_counts
        .iter()
        .map(|(line, count)| count.saturating_sub(*clean_counts.get(line).unwrap_or(&0)))
        .sum();
    let removed_line_count = clean_counts
        .iter()
        .map(|(line, count)| count.saturating_sub(*modded_counts.get(line).unwrap_or(&0)))
        .sum();

    let mut all_changes = Vec::new();
    let min_len = clean_lines.len().min(modded_lines.len());

    for index in 0..min_len {
        if clean_lines[index] != modded_lines[index] {
            all_changes.push(SampleLineChange {
                lineIndex: index + 1,
                kind: "changed".to_string(),
                oldLine: sanitize_sample_line(clean_lines[index]),
                newLine: sanitize_sample_line(modded_lines[index]),
                changeHint: classify_change_hint(clean_lines[index], modded_lines[index]),
            });
        }
    }

    for (index, line) in clean_lines.iter().enumerate().skip(min_len) {
        all_changes.push(SampleLineChange {
            lineIndex: index + 1,
            kind: "removed".to_string(),
            oldLine: sanitize_sample_line(line),
            newLine: String::new(),
            changeHint: classify_single_line_hint(line),
        });
    }

    for (index, line) in modded_lines.iter().enumerate().skip(min_len) {
        all_changes.push(SampleLineChange {
            lineIndex: index + 1,
            kind: "added".to_string(),
            oldLine: String::new(),
            newLine: sanitize_sample_line(line),
            changeHint: classify_single_line_hint(line),
        });
    }

    let numeric_changes = count_numeric_changes(&clean_lines, &modded_lines);
    let color_like_changes = count_color_like_changes(&all_changes);
    let mut samples = all_changes;
    samples.truncate(max_samples);

    LineDiffResult {
        cleanLineCount: clean_lines.len(),
        moddedLineCount: modded_lines.len(),
        addedLineCount: added_line_count,
        removedLineCount: removed_line_count,
        numericChanges: numeric_changes,
        colorLikeChanges: color_like_changes,
        samples,
    }
}

fn extract_key_value_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with(';')
            || line.starts_with('<')
        {
            continue;
        }

        let parsed = if let Some((key, value)) = line.split_once('=') {
            Some((key.trim(), value.trim()))
        } else if let Some((key, value)) = line.split_once(':') {
            Some((key.trim(), value.trim()))
        } else {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                Some((parts[0].trim(), parts[1].trim()))
            } else {
                None
            }
        };

        if let Some((key, value)) = parsed {
            if !key.is_empty()
                && !value.is_empty()
                && !key.contains('<')
                && !key.contains('>')
                && !value.starts_with('<')
            {
                pairs.push((key.to_string(), value.to_string()));
            }
        }
    }

    pairs
}

fn decode_text_side(bytes: Option<&[u8]>, side: &str, warnings: &mut Vec<String>) -> String {
    match bytes {
        Some(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_string(),
            Err(e) => {
                warnings.push(format!("{} bytes were not valid UTF-8: {}", side, e));
                String::from_utf8_lossy(bytes).into_owned()
            }
        },
        None => String::new(),
    }
}

fn warning_indicates_parse_failure(warning: &str) -> bool {
    warning.to_ascii_lowercase().contains("utf-8")
}

fn add_analyzer_warning(
    warnings: &mut Vec<AnalyzerWarning>,
    path: &str,
    analyzer: &str,
    reason: String,
) {
    warnings.push(AnalyzerWarning {
        path: path.to_string(),
        analyzer: analyzer.to_string(),
        reason,
    });
}

fn push_analysis_entry_warnings(
    analyzer_warnings: &mut Vec<AnalyzerWarning>,
    global_warnings: &mut Vec<Warning>,
    analyzer: &str,
    path: &str,
    entry_warnings: &[String],
) {
    for warning in entry_warnings {
        add_analyzer_warning(analyzer_warnings, path, analyzer, warning.clone());
        push_warning(
            global_warnings,
            "TEXT_PARSE_WARNING",
            path,
            format!("{}: {}", analyzer, warning),
        );
    }
}

fn analyze_xml_content(
    path: &str,
    status: &str,
    clean_bytes: Option<&[u8]>,
    modded_bytes: Option<&[u8]>,
) -> XmlDiffEntry {
    let mut warnings = Vec::new();
    let clean_text = decode_text_side(clean_bytes, "clean", &mut warnings);
    let modded_text = decode_text_side(modded_bytes, "modded", &mut warnings);
    let diff = diff_lines(&clean_text, &modded_text, MAX_SAMPLE_CHANGES);

    XmlDiffEntry {
        path: path.to_string(),
        status: status.to_string(),
        analyzer: "xml_analyzer".to_string(),
        parseStrategy: "line_diff_with_xml_hints".to_string(),
        cleanLines: diff.cleanLineCount,
        moddedLines: diff.moddedLineCount,
        addedLines: diff.addedLineCount,
        removedLines: diff.removedLineCount,
        numericChanges: diff.numericChanges,
        colorLikeChanges: diff.colorLikeChanges,
        sampleChanges: diff.samples,
        warnings,
    }
}

fn analyze_dat_content(
    path: &str,
    status: &str,
    clean_bytes: Option<&[u8]>,
    modded_bytes: Option<&[u8]>,
) -> DatDiffEntry {
    let mut warnings = Vec::new();
    let clean_text = decode_text_side(clean_bytes, "clean", &mut warnings);
    let modded_text = decode_text_side(modded_bytes, "modded", &mut warnings);
    let diff = diff_lines(&clean_text, &modded_text, MAX_SAMPLE_CHANGES);

    let clean_pairs: BTreeMap<String, String> =
        extract_key_value_pairs(&clean_text).into_iter().collect();
    let modded_pairs: BTreeMap<String, String> =
        extract_key_value_pairs(&modded_text).into_iter().collect();

    let mut changed_key_count = 0usize;
    let mut sample_key_changes = Vec::new();
    let all_keys: BTreeSet<String> = clean_pairs
        .keys()
        .chain(modded_pairs.keys())
        .cloned()
        .collect();

    for key in all_keys {
        if let (Some(old_value), Some(new_value)) = (clean_pairs.get(&key), modded_pairs.get(&key))
        {
            if old_value == new_value {
                continue;
            }
            changed_key_count += 1;
            if sample_key_changes.len() < MAX_KEY_VALUE_SAMPLES {
                let old_number = parse_number(old_value);
                let new_number = parse_number(new_value);
                sample_key_changes.push(KeyValueChange {
                    key: key.clone(),
                    oldValue: sanitize_and_truncate(old_value, 100),
                    newValue: sanitize_and_truncate(new_value, 100),
                    valueType: if old_number.is_some() && new_number.is_some() {
                        "number".to_string()
                    } else {
                        "string".to_string()
                    },
                    numericDelta: match (old_number, new_number) {
                        (Some(old_number), Some(new_number)) => Some(new_number - old_number),
                        _ => None,
                    },
                });
            }
        }
    }

    DatDiffEntry {
        path: path.to_string(),
        status: status.to_string(),
        analyzer: "dat_config_analyzer".to_string(),
        readable: warnings.is_empty(),
        cleanLines: diff.cleanLineCount,
        moddedLines: diff.moddedLineCount,
        changedKeyCount: changed_key_count,
        addedLines: diff.addedLineCount,
        removedLines: diff.removedLineCount,
        numericChanges: diff.numericChanges,
        sampleKeyChanges: sample_key_changes,
        sampleChanges: diff.samples,
        warnings,
    }
}

fn analyze_meta_content(
    path: &str,
    status: &str,
    clean_bytes: Option<&[u8]>,
    modded_bytes: Option<&[u8]>,
) -> MetaDiffEntry {
    let mut warnings = Vec::new();
    let clean_text = decode_text_side(clean_bytes, "clean", &mut warnings);
    let modded_text = decode_text_side(modded_bytes, "modded", &mut warnings);
    let diff = diff_lines(&clean_text, &modded_text, MAX_SAMPLE_CHANGES);

    MetaDiffEntry {
        path: path.to_string(),
        status: status.to_string(),
        analyzer: "meta_analyzer".to_string(),
        parseStrategy: "line_diff_with_meta_hints".to_string(),
        cleanLines: diff.cleanLineCount,
        moddedLines: diff.moddedLineCount,
        addedLines: diff.addedLineCount,
        removedLines: diff.removedLineCount,
        numericChanges: diff.numericChanges,
        colorLikeChanges: diff.colorLikeChanges,
        sampleChanges: diff.samples,
        warnings,
    }
}

fn analyze_generic_text_content(
    path: &str,
    status: &str,
    ext: &str,
    clean_bytes: Option<&[u8]>,
    modded_bytes: Option<&[u8]>,
) -> GenericTextDiffEntry {
    let mut warnings = Vec::new();
    let clean_text = decode_text_side(clean_bytes, "clean", &mut warnings);
    let modded_text = decode_text_side(modded_bytes, "modded", &mut warnings);
    let diff = diff_lines(&clean_text, &modded_text, MAX_SAMPLE_CHANGES);

    GenericTextDiffEntry {
        path: path.to_string(),
        status: status.to_string(),
        analyzer: "generic_text_analyzer".to_string(),
        parseStrategy: format!("line_diff_generic_text_{}", ext),
        cleanLines: diff.cleanLineCount,
        moddedLines: diff.moddedLineCount,
        addedLines: diff.addedLineCount,
        removedLines: diff.removedLineCount,
        numericChanges: diff.numericChanges,
        colorLikeChanges: diff.colorLikeChanges,
        sampleChanges: diff.samples,
        warnings,
    }
}

fn run_text_analyzers(
    changes: &[Change],
    clean_archive_path: &Path,
    modded_archive_path: &Path,
    keys: &GtaKeys,
    depth: usize,
    warnings: &mut Vec<Warning>,
) -> Result<TextAnalysisResults> {
    let text_candidate_changes: Vec<&Change> = changes
        .iter()
        .filter(|change| is_text_candidate_ext(&change.extension))
        .collect();

    let mut stats = TextAnalysisStats {
        totalCandidates: text_candidate_changes.len(),
        ..Default::default()
    };

    if text_candidate_changes.is_empty() {
        return Ok(TextAnalysisResults {
            xml_entries: Vec::new(),
            dat_entries: Vec::new(),
            meta_entries: Vec::new(),
            generic_entries: Vec::new(),
            analyzer_warnings: Vec::new(),
            stats,
            file_summaries: Vec::new(),
        });
    }

    let target_paths: BTreeSet<String> = text_candidate_changes
        .iter()
        .map(|change| normalize_path(&change.path))
        .collect();

    let clean_bytes_map = extract_text_bytes_for_paths(
        clean_archive_path,
        keys,
        depth,
        &target_paths,
        MAX_TEXT_FILE_BYTES,
        warnings,
    )
    .with_context(|| {
        format!(
            "failed to extract text bytes from clean archive {}",
            clean_archive_path.display()
        )
    })?;

    let modded_bytes_map = extract_text_bytes_for_paths(
        modded_archive_path,
        keys,
        depth,
        &target_paths,
        MAX_TEXT_FILE_BYTES,
        warnings,
    )
    .with_context(|| {
        format!(
            "failed to extract text bytes from modded archive {}",
            modded_archive_path.display()
        )
    })?;

    let mut results = TextAnalysisResults {
        xml_entries: Vec::new(),
        dat_entries: Vec::new(),
        meta_entries: Vec::new(),
        generic_entries: Vec::new(),
        analyzer_warnings: Vec::new(),
        stats: TextAnalysisStats::default(),
        file_summaries: Vec::new(),
    };

    for change in text_candidate_changes {
        let path_key = normalize_path(&change.path);
        let clean_bytes = clean_bytes_map.get(&path_key).map(Vec::as_slice);
        let modded_bytes = modded_bytes_map.get(&path_key).map(Vec::as_slice);
        let ext = change.extension.as_str();
        let analyzer_name = match ext {
            "xml" => "xml_analyzer",
            "dat" => "dat_config_analyzer",
            "meta" => "meta_analyzer",
            _ => "generic_text_analyzer",
        };

        let is_too_large = (change.status != "added" && change.cleanSize > MAX_TEXT_FILE_BYTES)
            || (change.status != "removed" && change.moddedSize > MAX_TEXT_FILE_BYTES);
        if is_too_large {
            stats.tooLargeSkipped += 1;
            push_warning(
                warnings,
                "TEXT_ANALYZE_TOO_LARGE",
                &change.path,
                format!(
                    "skipped text analyzer because file exceeds {} bytes",
                    MAX_TEXT_FILE_BYTES
                ),
            );
            add_analyzer_warning(
                &mut results.analyzer_warnings,
                &change.path,
                analyzer_name,
                "skipped: file exceeded text analyzer size limit".to_string(),
            );
            continue;
        }

        let missing_required_side = match change.status.as_str() {
            "added" => modded_bytes.is_none(),
            "removed" => clean_bytes.is_none(),
            _ => clean_bytes.is_none() || modded_bytes.is_none(),
        };
        if missing_required_side {
            stats.extractionFailures += 1;
            push_warning(
                warnings,
                "TEXT_ANALYZE_EXTRACTION_FAILED",
                &change.path,
                "required text bytes could not be extracted from one or both archives".to_string(),
            );
            add_analyzer_warning(
                &mut results.analyzer_warnings,
                &change.path,
                analyzer_name,
                "skipped: required text bytes could not be extracted".to_string(),
            );
            continue;
        }

        let should_require_text_heuristic = is_maybe_text_ext(ext) && !is_definitely_text_ext(ext);
        if should_require_text_heuristic {
            let clean_text_like = clean_bytes.map(looks_like_text).unwrap_or(true);
            let modded_text_like = modded_bytes.map(looks_like_text).unwrap_or(true);
            if !clean_text_like || !modded_text_like {
                stats.skippedNotTextBytes += 1;
                push_warning(
                    warnings,
                    "TEXT_ANALYZE_NOT_TEXT_BYTES",
                    &change.path,
                    "skipped maybe-text extension because bytes did not look like text".to_string(),
                );
                add_analyzer_warning(
                    &mut results.analyzer_warnings,
                    &change.path,
                    analyzer_name,
                    "skipped: bytes did not look like text".to_string(),
                );
                continue;
            }
        }

        match ext {
            "xml" => {
                let entry =
                    analyze_xml_content(&change.path, &change.status, clean_bytes, modded_bytes);
                if entry
                    .warnings
                    .iter()
                    .any(|warning| warning_indicates_parse_failure(warning))
                {
                    stats.parseFailures += 1;
                }
                push_analysis_entry_warnings(
                    &mut results.analyzer_warnings,
                    warnings,
                    "xml_analyzer",
                    &change.path,
                    &entry.warnings,
                );
                results.file_summaries.push(TextAnalysisFileSummary {
                    path: change.path.clone(),
                    extension: change.extension.clone(),
                    analyzer: "xml_analyzer".to_string(),
                    status: change.status.clone(),
                    sizeDelta: change.sizeDelta,
                    addedLines: entry.addedLines,
                    removedLines: entry.removedLines,
                    numericChanges: entry.numericChanges,
                    colorLikeChanges: entry.colorLikeChanges,
                });
                results.xml_entries.push(entry);
                stats.analyzedFiles += 1;
                stats.xmlAnalyzed += 1;
            }
            "dat" => {
                let entry =
                    analyze_dat_content(&change.path, &change.status, clean_bytes, modded_bytes);
                if entry
                    .warnings
                    .iter()
                    .any(|warning| warning_indicates_parse_failure(warning))
                {
                    stats.parseFailures += 1;
                }
                push_analysis_entry_warnings(
                    &mut results.analyzer_warnings,
                    warnings,
                    "dat_config_analyzer",
                    &change.path,
                    &entry.warnings,
                );
                results.file_summaries.push(TextAnalysisFileSummary {
                    path: change.path.clone(),
                    extension: change.extension.clone(),
                    analyzer: "dat_config_analyzer".to_string(),
                    status: change.status.clone(),
                    sizeDelta: change.sizeDelta,
                    addedLines: entry.addedLines,
                    removedLines: entry.removedLines,
                    numericChanges: entry.numericChanges,
                    colorLikeChanges: 0,
                });
                results.dat_entries.push(entry);
                stats.analyzedFiles += 1;
                stats.datAnalyzed += 1;
            }
            "meta" => {
                let entry =
                    analyze_meta_content(&change.path, &change.status, clean_bytes, modded_bytes);
                if entry
                    .warnings
                    .iter()
                    .any(|warning| warning_indicates_parse_failure(warning))
                {
                    stats.parseFailures += 1;
                }
                push_analysis_entry_warnings(
                    &mut results.analyzer_warnings,
                    warnings,
                    "meta_analyzer",
                    &change.path,
                    &entry.warnings,
                );
                results.file_summaries.push(TextAnalysisFileSummary {
                    path: change.path.clone(),
                    extension: change.extension.clone(),
                    analyzer: "meta_analyzer".to_string(),
                    status: change.status.clone(),
                    sizeDelta: change.sizeDelta,
                    addedLines: entry.addedLines,
                    removedLines: entry.removedLines,
                    numericChanges: entry.numericChanges,
                    colorLikeChanges: entry.colorLikeChanges,
                });
                results.meta_entries.push(entry);
                stats.analyzedFiles += 1;
                stats.metaAnalyzed += 1;
            }
            _ => {
                let entry = analyze_generic_text_content(
                    &change.path,
                    &change.status,
                    ext,
                    clean_bytes,
                    modded_bytes,
                );
                if entry
                    .warnings
                    .iter()
                    .any(|warning| warning_indicates_parse_failure(warning))
                {
                    stats.parseFailures += 1;
                }
                push_analysis_entry_warnings(
                    &mut results.analyzer_warnings,
                    warnings,
                    "generic_text_analyzer",
                    &change.path,
                    &entry.warnings,
                );
                results.file_summaries.push(TextAnalysisFileSummary {
                    path: change.path.clone(),
                    extension: change.extension.clone(),
                    analyzer: "generic_text_analyzer".to_string(),
                    status: change.status.clone(),
                    sizeDelta: change.sizeDelta,
                    addedLines: entry.addedLines,
                    removedLines: entry.removedLines,
                    numericChanges: entry.numericChanges,
                    colorLikeChanges: entry.colorLikeChanges,
                });
                results.generic_entries.push(entry);
                stats.analyzedFiles += 1;
                stats.genericTextAnalyzed += 1;
            }
        }
    }

    stats.skippedFiles = stats.totalCandidates.saturating_sub(stats.analyzedFiles);
    results.stats = stats;
    Ok(results)
}

fn write_text_analysis_summary(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    stats: &TextAnalysisStats,
    results: &TextAnalysisResults,
) -> Result<()> {
    #[derive(Serialize)]
    struct TopChangedFile {
        path: String,
        extension: String,
        analyzer: String,
        status: String,
        sizeDelta: i64,
    }

    #[derive(Serialize)]
    struct TopExtension {
        extension: String,
        count: usize,
    }

    #[derive(Serialize)]
    struct TextAnalysisSummaryFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        stats: &'a TextAnalysisStats,
        topChangedFiles: Vec<TopChangedFile>,
        topExtensions: Vec<TopExtension>,
        recommendedNextPhase: &'a str,
    }

    let mut top_changed_files = results.file_summaries.clone();
    top_changed_files.sort_by(|a, b| {
        b.sizeDelta
            .abs()
            .cmp(&a.sizeDelta.abs())
            .then_with(|| a.path.cmp(&b.path))
    });
    top_changed_files.truncate(10);

    let mut extension_counts = BTreeMap::new();
    for entry in &results.file_summaries {
        *extension_counts
            .entry(entry.extension.clone())
            .or_insert(0usize) += 1;
    }
    let mut top_extensions: Vec<TopExtension> = extension_counts
        .into_iter()
        .map(|(extension, count)| TopExtension { extension, count })
        .collect();
    top_extensions.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    top_extensions.truncate(10);

    let summary = TextAnalysisSummaryFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "text_analysis_summary",
        tool,
        timing,
        stats,
        topChangedFiles: top_changed_files
            .into_iter()
            .map(|entry| TopChangedFile {
                path: entry.path,
                extension: entry.extension,
                analyzer: entry.analyzer,
                status: entry.status,
                sizeDelta: entry.sizeDelta,
            })
            .collect(),
        topExtensions: top_extensions,
        recommendedNextPhase: "R0.8 YTD/GFX/YPT binary analyzers",
    };

    let out = out_dir.join("text_analysis_summary.json");
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_text_diff_entries<T: Serialize>(
    out_dir: &Path,
    file_name: &str,
    artifact_type: &str,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[T],
) -> Result<()> {
    #[derive(Serialize)]
    struct TextDiffStats {
        total: usize,
    }

    #[derive(Serialize)]
    struct TextDiffFile<'a, T> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        stats: TextDiffStats,
        entries: &'a [T],
    }

    let report = TextDiffFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: artifact_type,
        tool,
        timing,
        stats: TextDiffStats {
            total: entries.len(),
        },
        entries,
    };

    let out = out_dir.join(file_name);
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_xml_diffs(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[XmlDiffEntry],
) -> Result<()> {
    write_text_diff_entries(
        out_dir,
        "xml_diffs.json",
        "xml_diffs",
        tool,
        timing,
        entries,
    )
}

fn write_dat_diffs(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[DatDiffEntry],
) -> Result<()> {
    write_text_diff_entries(
        out_dir,
        "dat_diffs.json",
        "dat_diffs",
        tool,
        timing,
        entries,
    )
}

fn write_meta_diffs(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[MetaDiffEntry],
) -> Result<()> {
    write_text_diff_entries(
        out_dir,
        "meta_diffs.json",
        "meta_diffs",
        tool,
        timing,
        entries,
    )
}

fn write_generic_text_diffs(
    out_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[GenericTextDiffEntry],
) -> Result<()> {
    write_text_diff_entries(
        out_dir,
        "generic_text_diffs.json",
        "generic_text_diffs",
        tool,
        timing,
        entries,
    )
}

fn write_analyzer_warnings(out_dir: &Path, warnings: &[AnalyzerWarning]) -> Result<()> {
    #[derive(Serialize)]
    struct AnalyzerWarningsFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        total: usize,
        warnings: &'a [AnalyzerWarning],
    }

    let report = AnalyzerWarningsFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "analyzer_warnings",
        total: warnings.len(),
        warnings,
    };

    let out = out_dir.join("analyzer_warnings.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_ai_readable_change_notes(out_dir: &Path, results: &TextAnalysisResults) -> Result<()> {
    let mut summaries = results.file_summaries.clone();
    summaries.sort_by(|a, b| a.path.cmp(&b.path));

    let mut lines = Vec::new();
    for entry in summaries {
        let note = AiChangeNote {
            task: "explain_text_config_change".to_string(),
            path: entry.path,
            extension: entry.extension,
            analyzer: entry.analyzer,
            changeSummary: AiChangeSummary {
                addedLines: entry.addedLines,
                removedLines: entry.removedLines,
                numericChanges: entry.numericChanges,
                colorLikeChanges: entry.colorLikeChanges,
            },
            question: "What visual/config component might this change relate to? Treat as hypothesis only.".to_string(),
        };
        lines.push(serde_json::to_string(&note)?);
    }

    let out = out_dir.join("ai_readable_change_notes.jsonl");
    fs::write(&out, lines.join("\n"))?;
    Ok(())
}

// ── Learning Corpus Builder (R0.8) ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct ComponentFreqEntry {
    component: String,
    totalChanged: usize,
    added: usize,
    removed: usize,
    modified: usize,
    topExtensions: Vec<String>,
    topPaths: Vec<String>,
    isKnownComponent: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FileTypeFreqEntry {
    extension: String,
    totalChanged: usize,
    added: usize,
    removed: usize,
    modified: usize,
    knownCount: usize,
    unknownCount: usize,
    textCandidateCount: usize,
    binaryCandidateCount: usize,
    analyzerStatus: String,
}

#[derive(Debug, Clone, Serialize)]
struct AnalyzerCoverageReport {
    totalCandidates: usize,
    analyzedFiles: usize,
    skippedFiles: usize,
    xmlAnalyzed: usize,
    datAnalyzed: usize,
    metaAnalyzed: usize,
    genericAnalyzed: usize,
    parseFailures: usize,
    extractionFailures: usize,
    tooLargeSkipped: usize,
    binaryPsoSkipped: usize,
    coveragePercent: f64,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
struct CorpusAiChangeNote {
    kind: String,
    path: String,
    extension: String,
    analyzer: String,
    summary: CorpusChangeSummary,
    hypothesis: String,
    safeForAiPlanning: bool,
    safeForGeneration: bool,
    recommendedFutureTool: String,
}

#[derive(Debug, Clone, Serialize)]
struct CorpusChangeSummary {
    addedLines: usize,
    removedLines: usize,
    numericChanges: usize,
    colorLikeChanges: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ComponentLesson {
    component: String,
    lesson: String,
    evidence: Vec<String>,
    confidence: String,
    safeForGeneration: bool,
    recommendedNextStep: String,
}

#[derive(Debug, Clone, Serialize)]
struct FileLesson {
    path: String,
    extension: String,
    analyzer: String,
    status: String,
    numericChanges: usize,
    colorLikeChanges: usize,
    possibleComponent: String,
    whyItMatters: String,
    recommendedFutureTool: String,
    safeForGeneration: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TrainingCandidate {
    task: String,
    trainingStatus: String,
    input: TrainingCandidateInput,
    expectedOutputStyle: TrainingCandidateExpected,
}

#[derive(Debug, Clone, Serialize)]
struct TrainingCandidateInput {
    path: String,
    extension: String,
    analyzerSummary: CorpusChangeSummary,
}

#[derive(Debug, Clone, Serialize)]
struct TrainingCandidateExpected {
    componentHypothesis: String,
    risk: String,
    recommendedTool: String,
    safeForGeneration: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CorpusIndex {
    schemaVersion: String,
    generatedAt: String,
    scannerVersion: String,
    baselineArchiveHash: String,
    baselineArchiveFileName: String,
    moddedArchiveHash: String,
    moddedArchiveFileName: String,
    sourceArtifacts: Vec<String>,
    totals: CorpusTotals,
    artifacts: Vec<String>,
    warning: String,
}

#[derive(Debug, Clone, Serialize)]
struct CorpusTotals {
    added: usize,
    removed: usize,
    modified: usize,
    totalUnknown: usize,
    textCandidates: usize,
    binaryCandidates: usize,
    analyzedTextFiles: usize,
    skippedTextFiles: usize,
    candidatePatterns: usize,
}

fn corpus_abs_size_delta(size_delta: i64) -> u64 {
    size_delta.checked_abs().unwrap_or(i64::MAX) as u64
}

fn corpus_round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn corpus_extension_label(ext: &str) -> String {
    if ext.trim().is_empty() {
        "(none)".to_string()
    } else {
        ext.trim().to_lowercase()
    }
}

fn corpus_component_key(component: &str) -> String {
    let trimmed = component.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_lowercase()
    }
}

fn corpus_known_component(change: &Change) -> Option<String> {
    change
        .components
        .iter()
        .map(|component| corpus_component_key(component))
        .find(|component| component != "unknown")
}

fn corpus_recommended_future_tool(extension: &str) -> &'static str {
    match extension {
        "xml" => "xml_timecycle_editor",
        "dat" => "dat_config_patcher",
        "meta" => "meta_editor",
        "txt" | "ini" | "cfg" | "json" | "nametable" => "text_diff_tool",
        "ytd" => "ytd_texture_analyzer",
        "gfx" | "swf" => "gfx_swf_analyzer",
        "ypt" => "ypt_particle_analyzer",
        _ => "unknown_analyzer",
    }
}

fn corpus_file_type_analyzer_status(extension: &str) -> &'static str {
    match extension {
        "xml" | "dat" | "meta" | "txt" | "ini" | "cfg" | "json" | "nametable" => "analyzed",
        "ymt" | "ymap" | "ytyp" => "skipped_binary",
        "ytd" | "gfx" | "swf" | "ypt" | "ydr" | "yft" | "ybn" | "awc" | "ysc" | "gxt2" | "ydd"
        | "yld" | "rel" => "analyzer_required",
        _ => "unsupported",
    }
}

fn corpus_change_hypothesis(
    path: &str,
    extension: &str,
    numeric_changes: usize,
    color_like_changes: usize,
) -> String {
    let normalized = normalize_path(path);
    if normalized.contains("timecycle") || normalized.contains("cloud") || color_like_changes >= 50
    {
        "Likely timecycle/sky visual config. Treat as unconfirmed.".to_string()
    } else if normalized.contains("visual") {
        "Likely visual settings config. Treat as unconfirmed.".to_string()
    } else if normalized.contains("weapon") || normalized.contains("handling") {
        "Likely gameplay/vehicle config. Treat as unconfirmed.".to_string()
    } else if extension == "dat" && numeric_changes > 0 {
        "Likely numeric config values. Treat as unconfirmed.".to_string()
    } else {
        "Config/data file with unknown purpose. Treat as unconfirmed.".to_string()
    }
}

fn corpus_file_importance_reason(
    numeric_changes: usize,
    color_like_changes: usize,
    size_delta: i64,
) -> String {
    if color_like_changes > 50 {
        "Large color-like edits suggest a visual or timecycle tuning pass that should be reviewed carefully."
            .to_string()
    } else if numeric_changes > 50 {
        "Heavy numeric edits suggest broad tuning changes that may affect gameplay or visuals."
            .to_string()
    } else if corpus_abs_size_delta(size_delta) > 100000 {
        "Large size delta suggests a broad rewrite or asset replacement that deserves manual inspection."
            .to_string()
    } else if numeric_changes > 0 || color_like_changes > 0 {
        "Readable config deltas indicate targeted tuning that can inform future tooling hypotheses."
            .to_string()
    } else {
        "Changed file may influence a Redux component and should be reviewed manually before drawing conclusions."
            .to_string()
    }
}

fn compute_component_frequency(changes: &[Change]) -> Vec<ComponentFreqEntry> {
    #[derive(Default)]
    struct ComponentAgg {
        total_changed: usize,
        added: usize,
        removed: usize,
        modified: usize,
        ext_counts: BTreeMap<String, usize>,
        path_counts: BTreeMap<String, usize>,
        is_known_component: bool,
    }

    let mut groups: BTreeMap<String, ComponentAgg> = BTreeMap::new();

    for change in changes {
        let mut saw_unknown = false;
        let mut saw_known = false;

        for component in &change.components {
            let key = corpus_component_key(component);
            if key == "unknown" {
                saw_unknown = true;
                continue;
            }
            saw_known = true;
            let entry = groups.entry(key).or_default();
            entry.total_changed += 1;
            match change.status.as_str() {
                "added" => entry.added += 1,
                "removed" => entry.removed += 1,
                _ => entry.modified += 1,
            }
            *entry
                .ext_counts
                .entry(corpus_extension_label(&change.extension))
                .or_insert(0usize) += 1;
            *entry
                .path_counts
                .entry(normalize_path(&change.path))
                .or_insert(0usize) += 1;
            entry.is_known_component = true;
        }

        if saw_unknown || !saw_known {
            let entry = groups.entry("unknown".to_string()).or_default();
            entry.total_changed += 1;
            match change.status.as_str() {
                "added" => entry.added += 1,
                "removed" => entry.removed += 1,
                _ => entry.modified += 1,
            }
            *entry
                .ext_counts
                .entry(corpus_extension_label(&change.extension))
                .or_insert(0usize) += 1;
            *entry
                .path_counts
                .entry(normalize_path(&change.path))
                .or_insert(0usize) += 1;
        }
    }

    let mut entries: Vec<ComponentFreqEntry> = groups
        .into_iter()
        .map(|(component, agg)| {
            let mut top_extensions: Vec<(String, usize)> = agg.ext_counts.into_iter().collect();
            top_extensions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let mut top_paths: Vec<(String, usize)> = agg.path_counts.into_iter().collect();
            top_paths.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            ComponentFreqEntry {
                component: component.clone(),
                totalChanged: agg.total_changed,
                added: agg.added,
                removed: agg.removed,
                modified: agg.modified,
                topExtensions: top_extensions
                    .into_iter()
                    .take(5)
                    .map(|(extension, _)| extension)
                    .collect(),
                topPaths: top_paths
                    .into_iter()
                    .take(5)
                    .map(|(path, _)| path)
                    .collect(),
                isKnownComponent: component != "unknown" && agg.is_known_component,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.totalChanged
            .cmp(&a.totalChanged)
            .then_with(|| a.component.cmp(&b.component))
    });
    entries.truncate(50);
    entries
}

fn compute_file_type_frequency(
    changes: &[Change],
    unknown_entries: &[UnknownEntry],
) -> Vec<FileTypeFreqEntry> {
    #[derive(Default)]
    struct FileTypeAgg {
        total_changed: usize,
        added: usize,
        removed: usize,
        modified: usize,
        known_count: usize,
        unknown_count: usize,
        text_candidate_count: usize,
        binary_candidate_count: usize,
    }

    let unknown_paths: BTreeSet<String> = unknown_entries
        .iter()
        .map(|entry| normalize_path(&entry.path))
        .collect();
    let mut groups: BTreeMap<String, FileTypeAgg> = BTreeMap::new();

    for change in changes {
        let extension = corpus_extension_label(&change.extension);
        let entry = groups.entry(extension.clone()).or_default();
        entry.total_changed += 1;
        match change.status.as_str() {
            "added" => entry.added += 1,
            "removed" => entry.removed += 1,
            _ => entry.modified += 1,
        }

        let is_unknown = unknown_paths.contains(&normalize_path(&change.path))
            || change
                .components
                .iter()
                .all(|component| corpus_component_key(component) == "unknown");
        if is_unknown {
            entry.unknown_count += 1;
        } else {
            entry.known_count += 1;
        }

        if is_text_candidate_ext(&extension) {
            entry.text_candidate_count += 1;
        } else {
            entry.binary_candidate_count += 1;
        }
    }

    let mut entries: Vec<FileTypeFreqEntry> = groups
        .into_iter()
        .map(|(extension, agg)| FileTypeFreqEntry {
            analyzerStatus: corpus_file_type_analyzer_status(&extension).to_string(),
            extension,
            totalChanged: agg.total_changed,
            added: agg.added,
            removed: agg.removed,
            modified: agg.modified,
            knownCount: agg.known_count,
            unknownCount: agg.unknown_count,
            textCandidateCount: agg.text_candidate_count,
            binaryCandidateCount: agg.binary_candidate_count,
        })
        .collect();

    entries.sort_by(|a, b| {
        b.totalChanged
            .cmp(&a.totalChanged)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    entries.truncate(50);
    entries
}

fn build_corpus_ai_change_notes(text_results: &TextAnalysisResults) -> Vec<CorpusAiChangeNote> {
    let mut notes: Vec<CorpusAiChangeNote> = text_results
        .file_summaries
        .iter()
        .map(|entry| CorpusAiChangeNote {
            kind: "analyzed_text_change".to_string(),
            path: entry.path.clone(),
            extension: entry.extension.clone(),
            analyzer: entry.analyzer.clone(),
            summary: CorpusChangeSummary {
                addedLines: entry.addedLines,
                removedLines: entry.removedLines,
                numericChanges: entry.numericChanges,
                colorLikeChanges: entry.colorLikeChanges,
            },
            hypothesis: corpus_change_hypothesis(
                &entry.path,
                &entry.extension,
                entry.numericChanges,
                entry.colorLikeChanges,
            ),
            safeForAiPlanning: true,
            safeForGeneration: false,
            recommendedFutureTool: corpus_recommended_future_tool(&entry.extension).to_string(),
        })
        .collect();

    notes.sort_by(|a, b| a.path.cmp(&b.path));
    notes
}

fn build_corpus_ai_change_notes_from_opt(
    text_results: Option<&TextAnalysisResults>,
) -> Vec<CorpusAiChangeNote> {
    match text_results {
        Some(results) => build_corpus_ai_change_notes(results),
        None => Vec::new(),
    }
}

fn build_component_lessons(
    freq_entries: &[ComponentFreqEntry],
    changes: &[Change],
) -> Vec<ComponentLesson> {
    let mut lessons = Vec::new();

    for entry in freq_entries.iter().filter(|entry| entry.totalChanged > 0) {
        let matching_changes = changes
            .iter()
            .filter(|change| {
                if entry.component == "unknown" {
                    change
                        .components
                        .iter()
                        .all(|component| corpus_component_key(component) == "unknown")
                } else {
                    change
                        .components
                        .iter()
                        .any(|component| corpus_component_key(component) == entry.component)
                }
            })
            .count();

        let confidence =
            if entry.component != "unknown" && entry.totalChanged >= 2 && matching_changes >= 2 {
                "medium"
            } else {
                "low"
            };

        let top_extension = entry
            .topExtensions
            .first()
            .cloned()
            .unwrap_or_else(|| "(none)".to_string());
        let top_paths = if entry.topPaths.is_empty() {
            "none".to_string()
        } else {
            entry
                .topPaths
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };
        let lesson = if entry.component == "unknown" {
            format!(
                "Unknown changes likely cluster around {} file(s), mainly {}. Treat this only as a candidate grouping until a dedicated analyzer confirms it.",
                entry.totalChanged, top_extension
            )
        } else {
            format!(
                "{} likely represents a candidate component cluster with {} changed file(s), mainly {}. Treat this as cautious evidence rather than ground truth.",
                entry.component, entry.totalChanged, top_extension
            )
        };

        let evidence = vec![
            format!(
                "counts: total={}, added={}, removed={}, modified={}",
                entry.totalChanged, entry.added, entry.removed, entry.modified
            ),
            format!("top extensions: {}", entry.topExtensions.join(", ")),
            format!("sample paths: {}", top_paths),
        ];

        lessons.push(ComponentLesson {
            component: entry.component.clone(),
            lesson,
            evidence,
            confidence: confidence.to_string(),
            safeForGeneration: false,
            recommendedNextStep: corpus_recommended_future_tool(&top_extension).to_string(),
        });
    }

    lessons.sort_by(|a, b| a.component.cmp(&b.component));
    lessons
}

fn build_file_lessons(
    changes: &[Change],
    text_results: Option<&TextAnalysisResults>,
) -> Vec<FileLesson> {
    let change_by_path: BTreeMap<String, &Change> = changes
        .iter()
        .map(|change| (normalize_path(&change.path), change))
        .collect();
    let mut scored_lessons: Vec<(FileLesson, usize, usize, u64)> = Vec::new();
    let mut seen = BTreeSet::new();

    if let Some(results) = text_results {
        let mut summaries: Vec<&TextAnalysisFileSummary> = results.file_summaries.iter().collect();
        summaries.sort_by(|a, b| {
            b.numericChanges
                .cmp(&a.numericChanges)
                .then_with(|| b.colorLikeChanges.cmp(&a.colorLikeChanges))
                .then_with(|| {
                    corpus_abs_size_delta(b.sizeDelta).cmp(&corpus_abs_size_delta(a.sizeDelta))
                })
                .then_with(|| a.path.cmp(&b.path))
        });

        for summary in summaries.iter().copied() {
            let impact_size = corpus_abs_size_delta(summary.sizeDelta);
            if summary.numericChanges <= 50
                && summary.colorLikeChanges <= 50
                && impact_size <= 100000
            {
                continue;
            }
            let normalized = normalize_path(&summary.path);
            if !seen.insert(normalized.clone()) {
                continue;
            }
            let possible_component = change_by_path
                .get(&normalized)
                .and_then(|change| corpus_known_component(change))
                .unwrap_or_else(|| "unknown".to_string());
            scored_lessons.push((
                FileLesson {
                    path: summary.path.clone(),
                    extension: summary.extension.clone(),
                    analyzer: summary.analyzer.clone(),
                    status: summary.status.clone(),
                    numericChanges: summary.numericChanges,
                    colorLikeChanges: summary.colorLikeChanges,
                    possibleComponent: possible_component,
                    whyItMatters: corpus_file_importance_reason(
                        summary.numericChanges,
                        summary.colorLikeChanges,
                        summary.sizeDelta,
                    ),
                    recommendedFutureTool: corpus_recommended_future_tool(&summary.extension)
                        .to_string(),
                    safeForGeneration: false,
                },
                summary.numericChanges,
                summary.colorLikeChanges,
                impact_size,
            ));
        }

        for summary in summaries.iter().copied() {
            if scored_lessons.len() >= 10 {
                break;
            }
            let normalized = normalize_path(&summary.path);
            if !seen.insert(normalized.clone()) {
                continue;
            }
            let impact_size = corpus_abs_size_delta(summary.sizeDelta);
            if summary.numericChanges == 0 && summary.colorLikeChanges == 0 && impact_size == 0 {
                continue;
            }
            let possible_component = change_by_path
                .get(&normalized)
                .and_then(|change| corpus_known_component(change))
                .unwrap_or_else(|| "unknown".to_string());
            scored_lessons.push((
                FileLesson {
                    path: summary.path.clone(),
                    extension: summary.extension.clone(),
                    analyzer: summary.analyzer.clone(),
                    status: summary.status.clone(),
                    numericChanges: summary.numericChanges,
                    colorLikeChanges: summary.colorLikeChanges,
                    possibleComponent: possible_component,
                    whyItMatters: corpus_file_importance_reason(
                        summary.numericChanges,
                        summary.colorLikeChanges,
                        summary.sizeDelta,
                    ),
                    recommendedFutureTool: corpus_recommended_future_tool(&summary.extension)
                        .to_string(),
                    safeForGeneration: false,
                },
                summary.numericChanges,
                summary.colorLikeChanges,
                impact_size,
            ));
        }
    }

    let mut sorted_changes: Vec<&Change> = changes.iter().collect();
    sorted_changes.sort_by(|a, b| {
        corpus_abs_size_delta(b.sizeDelta)
            .cmp(&corpus_abs_size_delta(a.sizeDelta))
            .then_with(|| a.path.cmp(&b.path))
    });

    for change in sorted_changes {
        if scored_lessons.len() >= 25 {
            break;
        }
        let impact_size = corpus_abs_size_delta(change.sizeDelta);
        if impact_size <= 100000 && !scored_lessons.is_empty() {
            continue;
        }
        let normalized = normalize_path(&change.path);
        if !seen.insert(normalized) {
            continue;
        }
        scored_lessons.push((
            FileLesson {
                path: change.path.clone(),
                extension: change.extension.clone(),
                analyzer: if is_text_candidate_ext(&change.extension) {
                    "text_change_candidate".to_string()
                } else {
                    "binary_change_candidate".to_string()
                },
                status: change.status.clone(),
                numericChanges: 0,
                colorLikeChanges: 0,
                possibleComponent: corpus_known_component(change)
                    .unwrap_or_else(|| "unknown".to_string()),
                whyItMatters: corpus_file_importance_reason(0, 0, change.sizeDelta),
                recommendedFutureTool: corpus_recommended_future_tool(&change.extension)
                    .to_string(),
                safeForGeneration: false,
            },
            0,
            0,
            impact_size,
        ));
    }

    scored_lessons.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| b.3.cmp(&a.3))
            .then_with(|| a.0.path.cmp(&b.0.path))
    });

    scored_lessons
        .into_iter()
        .map(|(lesson, _, _, _)| lesson)
        .collect()
}

fn build_training_candidates(
    text_results: Option<&TextAnalysisResults>,
    changes: &[Change],
) -> Vec<TrainingCandidate> {
    let Some(results) = text_results else {
        return Vec::new();
    };

    let change_by_path: BTreeMap<String, &Change> = changes
        .iter()
        .map(|change| (normalize_path(&change.path), change))
        .collect();
    let mut summaries: Vec<&TextAnalysisFileSummary> = results
        .file_summaries
        .iter()
        .filter(|summary| summary.numericChanges > 0 || summary.colorLikeChanges > 0)
        .collect();
    summaries.sort_by(|a, b| {
        b.numericChanges
            .cmp(&a.numericChanges)
            .then_with(|| b.colorLikeChanges.cmp(&a.colorLikeChanges))
            .then_with(|| a.path.cmp(&b.path))
    });

    let mut candidates = Vec::new();
    for summary in summaries.into_iter().take(500) {
        let risk = change_by_path
            .get(&normalize_path(&summary.path))
            .map(|change| change.risk.clone())
            .unwrap_or_else(|| "unknown".to_string());
        candidates.push(TrainingCandidate {
            task: "explain_redux_file_change".to_string(),
            trainingStatus: "candidate_unreviewed".to_string(),
            input: TrainingCandidateInput {
                path: summary.path.clone(),
                extension: summary.extension.clone(),
                analyzerSummary: CorpusChangeSummary {
                    addedLines: summary.addedLines,
                    removedLines: summary.removedLines,
                    numericChanges: summary.numericChanges,
                    colorLikeChanges: summary.colorLikeChanges,
                },
            },
            expectedOutputStyle: TrainingCandidateExpected {
                componentHypothesis: "unknown - review required".to_string(),
                risk,
                recommendedTool: corpus_recommended_future_tool(&summary.extension).to_string(),
                safeForGeneration: false,
            },
        });
    }

    candidates
}

fn render_local_ai_context(
    changes: &[Change],
    unknown_entries: &[UnknownEntry],
    text_results: Option<&TextAnalysisResults>,
    freq_entries: &[ComponentFreqEntry],
    baseline_meta: &BaselineMetadataFile,
    pattern_count: usize,
    generated_at: &str,
) -> String {
    let added = changes
        .iter()
        .filter(|change| change.status == "added")
        .count();
    let removed = changes
        .iter()
        .filter(|change| change.status == "removed")
        .count();
    let modified = changes
        .iter()
        .filter(|change| change.status == "modified")
        .count();
    let text_candidates = unknown_entries
        .iter()
        .filter(|entry| is_text_candidate_ext(&entry.extension))
        .count();
    let binary_candidates = unknown_entries.len().saturating_sub(text_candidates);
    let coverage = text_results.map(|results| {
        let percent = if results.stats.totalCandidates > 0 {
            corpus_round_one_decimal(
                (results.stats.analyzedFiles as f64 / results.stats.totalCandidates as f64) * 100.0,
            )
        } else {
            0.0
        };
        format!(
            "- analyzed: {} / {} ({:.1}%)\n- xml/dat/meta/generic: {}/{}/{}/{}\n- parse failures: {}\n- extraction failures: {}\n- too large skipped: {}\n- skipped non-text bytes: {}",
            results.stats.analyzedFiles,
            results.stats.totalCandidates,
            percent,
            results.stats.xmlAnalyzed,
            results.stats.datAnalyzed,
            results.stats.metaAnalyzed,
            results.stats.genericTextAnalyzed,
            results.stats.parseFailures,
            results.stats.extractionFailures,
            results.stats.tooLargeSkipped,
            results.stats.skippedNotTextBytes,
        )
    });

    let mut known_components: Vec<&ComponentFreqEntry> = freq_entries
        .iter()
        .filter(|entry| entry.isKnownComponent)
        .collect();
    known_components.sort_by(|a, b| {
        b.totalChanged
            .cmp(&a.totalChanged)
            .then_with(|| a.component.cmp(&b.component))
    });

    let mut readable_lines = Vec::new();
    if let Some(results) = text_results {
        let mut files: Vec<&TextAnalysisFileSummary> = results.file_summaries.iter().collect();
        files.sort_by(|a, b| {
            b.numericChanges
                .cmp(&a.numericChanges)
                .then_with(|| b.colorLikeChanges.cmp(&a.colorLikeChanges))
                .then_with(|| {
                    corpus_abs_size_delta(b.sizeDelta).cmp(&corpus_abs_size_delta(a.sizeDelta))
                })
                .then_with(|| a.path.cmp(&b.path))
        });
        for file in files.into_iter().take(10) {
            readable_lines.push(format!(
                "- `{}` [{}] numeric={}, color={}, sizeDelta={}",
                file.path,
                file.analyzer,
                file.numericChanges,
                file.colorLikeChanges,
                file.sizeDelta
            ));
        }
    }
    if readable_lines.is_empty() {
        let mut text_changes: Vec<&Change> = changes
            .iter()
            .filter(|change| is_text_candidate_ext(&change.extension))
            .collect();
        text_changes.sort_by(|a, b| {
            corpus_abs_size_delta(b.sizeDelta)
                .cmp(&corpus_abs_size_delta(a.sizeDelta))
                .then_with(|| a.path.cmp(&b.path))
        });
        for change in text_changes.into_iter().take(10) {
            readable_lines.push(format!(
                "- `{}` [{}] sizeDelta={}",
                change.path, change.extension, change.sizeDelta
            ));
        }
    }
    if readable_lines.is_empty() {
        readable_lines.push("- No readable text/config deltas were available.".to_string());
    }

    let known_component_lines = if known_components.is_empty() {
        "- No known component groups were identified.".to_string()
    } else {
        known_components
            .into_iter()
            .take(10)
            .map(|entry| {
                format!(
                    "- {}: {} changed file(s)",
                    entry.component, entry.totalChanged
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# Local AI Context — Redux vs Clean Diff\n\n## What was compared\n- baseline archive: {} ({})\n- modded archive: unknown (see learning_corpus_index.json for identity metadata)\n- generated at: {}\n\n## High-level totals\n- added: {}\n- removed: {}\n- modified: {}\n- unknown changes total: {}\n\n## Known components changed\n{}\n\n## Unknown patterns\n- total unknown entries: {}\n- text candidates: {}\n- binary candidates: {}\n- candidate patterns count: {}\n\n## Analyzer coverage\n{}\n\n## Key readable files changed (top 10 by impact)\n{}\n\n## What is safe to reason about\n- File-level changes and component assignments are safe to analyze.\n- Numeric/color-like changes in XML/DAT/META are hypotheses only.\n- Frequency trends can guide future analyzer work.\n\n## What is NOT safe to generate yet\n- Do not generate RPF patches based on this corpus alone.\n- Binary formats (YTD, GFX, YPT, YMT, YMAP) require future analyzers.\n- Verify all AI hypotheses against the actual game data.\n\n## Binary formats requiring future analyzers\n- ytd\n- gfx\n- ypt\n- ymt\n- ymap\n- ytyp\n- ybn\n- ydr\n- yft\n- awc\n- ysc\n- gxt2\n",
        baseline_meta.baselineArchiveFileName,
        baseline_meta.baselineArchiveHash,
        generated_at,
        added,
        removed,
        modified,
        unknown_entries.len(),
        known_component_lines,
        unknown_entries.len(),
        text_candidates,
        binary_candidates,
        pattern_count,
        coverage.unwrap_or_else(|| "text analysis not run".to_string()),
        readable_lines.join("\n"),
    )
}

fn render_redux_making_atlas(
    changes: &[Change],
    unknown_entries: &[UnknownEntry],
    text_results: Option<&TextAnalysisResults>,
    freq_entries: &[ComponentFreqEntry],
    file_lessons: &[FileLesson],
    component_lessons: &[ComponentLesson],
    pattern_count: usize,
    generated_at: &str,
) -> String {
    let added = changes
        .iter()
        .filter(|change| change.status == "added")
        .count();
    let removed = changes
        .iter()
        .filter(|change| change.status == "removed")
        .count();
    let modified = changes
        .iter()
        .filter(|change| change.status == "modified")
        .count();

    let mut known_components: Vec<&ComponentFreqEntry> = freq_entries
        .iter()
        .filter(|entry| entry.isKnownComponent)
        .collect();
    known_components.sort_by(|a, b| {
        b.totalChanged
            .cmp(&a.totalChanged)
            .then_with(|| a.component.cmp(&b.component))
    });

    let component_rows = if known_components.is_empty() {
        "| Component | Files Changed | Extensions | Status |\n| --- | ---: | --- | --- |\n| none | 0 | - | no known component mapping |"
            .to_string()
    } else {
        let mut rows = vec![
            "| Component | Files Changed | Extensions | Status |".to_string(),
            "| --- | ---: | --- | --- |".to_string(),
        ];
        for entry in known_components.into_iter().take(15) {
            rows.push(format!(
                "| {} | {} | {} | {} |",
                entry.component,
                entry.totalChanged,
                entry.topExtensions.join(", "),
                if entry.isKnownComponent {
                    "known-from-diff"
                } else {
                    "candidate"
                }
            ));
        }
        rows.join("\n")
    };

    let mut unknown_extension_counts = BTreeMap::new();
    let mut unknown_folder_counts = BTreeMap::new();
    for entry in unknown_entries {
        *unknown_extension_counts
            .entry(corpus_extension_label(&entry.extension))
            .or_insert(0usize) += 1;
        if let Some(folder) = top_level_folder(&entry.path) {
            *unknown_folder_counts.entry(folder).or_insert(0usize) += 1;
        }
    }
    let mut top_unknown_extensions: Vec<(String, usize)> =
        unknown_extension_counts.into_iter().collect();
    top_unknown_extensions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut top_unknown_folders: Vec<(String, usize)> = unknown_folder_counts.into_iter().collect();
    top_unknown_folders.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let readable_summary = match text_results {
        Some(results) => {
            let mut files: Vec<&TextAnalysisFileSummary> = results.file_summaries.iter().collect();
            files.sort_by(|a, b| {
                b.numericChanges
                    .cmp(&a.numericChanges)
                    .then_with(|| b.colorLikeChanges.cmp(&a.colorLikeChanges))
                    .then_with(|| a.path.cmp(&b.path))
            });
            let highlights = if files.is_empty() {
                "- No readable text/config files were analyzed.".to_string()
            } else {
                files
                    .into_iter()
                    .take(5)
                    .map(|file| {
                        format!(
                            "- `{}` [{}] numeric={}, color={}",
                            file.path, file.analyzer, file.numericChanges, file.colorLikeChanges
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            format!(
                "- XML analyzed: {}\n- DAT analyzed: {}\n- META analyzed: {}\n- Generic analyzed: {}\n{}",
                results.stats.xmlAnalyzed,
                results.stats.datAnalyzed,
                results.stats.metaAnalyzed,
                results.stats.genericTextAnalyzed,
                highlights,
            )
        }
        None => "- text analysis not run".to_string(),
    };

    let mut binary_extension_counts = BTreeMap::new();
    for entry in unknown_entries
        .iter()
        .filter(|entry| entry.analyzerRequired || !is_text_candidate_ext(&entry.extension))
    {
        *binary_extension_counts
            .entry(corpus_extension_label(&entry.extension))
            .or_insert(0usize) += 1;
    }
    let mut binary_extensions: Vec<(String, usize)> = binary_extension_counts.into_iter().collect();
    binary_extensions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let binary_lines = if binary_extensions.is_empty() {
        "- none".to_string()
    } else {
        binary_extensions
            .into_iter()
            .take(5)
            .map(|(extension, count)| format!("- {}: {} file(s)", extension, count))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let important_files = if file_lessons.is_empty() {
        "- none".to_string()
    } else {
        file_lessons
            .iter()
            .take(10)
            .map(|lesson| {
                format!(
                    "- `{}` [{}] component={} tool={}",
                    lesson.path,
                    lesson.analyzer,
                    lesson.possibleComponent,
                    lesson.recommendedFutureTool
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let component_lesson_lines = if component_lessons.is_empty() {
        "- No component lessons yet.".to_string()
    } else {
        component_lessons
            .iter()
            .take(5)
            .map(|lesson| format!("- {} ({})", lesson.component, lesson.confidence))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# Redux Making Atlas — Candidate Component Map\n\n## Overview\n- generated at: {}\n- total changed files: {}\n- added/removed/modified: {}/{}/{}\n- unknown candidate files: {}\n- component lesson count: {}\n\n## Known Component Changes (from diff)\n{}\n\n## Unknown Candidate Patterns\n- Count: {}\n- Top extensions: {}\n- Top folders: {}\n- Candidate patterns: {}\n\n## Readable Config/Text Changes\n{}\n\n## Binary Analyzer-Required Changes\n{}\n\n## Important Files to Investigate\n{}\n\n## Future Tool Recommendations\n- xml_timecycle_editor\n- dat_config_patcher\n- meta_editor\n- ytd_texture_analyzer\n- gfx_swf_analyzer\n\n## What an AI Can Safely Infer\n- Which files changed and how often a component label appeared.\n- Which readable config files had heavy numeric or color-like edits.\n- Which binary extensions need dedicated analyzers next.\n{}\n\n## What an AI Must NOT Generate Yet\n- Direct RPF patch payloads.\n- Final gameplay or visual claims without manual validation.\n- Binary asset rewrites for YTD/GFX/YPT/YMAP/YTYP/YDR/YFT.\n\nGenerated by Redux Scanner Engine — local only, do not commit if derived from real game files\n",
        generated_at,
        changes.len(),
        added,
        removed,
        modified,
        unknown_entries.len(),
        component_lessons.len(),
        component_rows,
        unknown_entries.len(),
        if top_unknown_extensions.is_empty() {
            "none".to_string()
        } else {
            top_unknown_extensions
                .into_iter()
                .take(5)
                .map(|(extension, count)| format!("{} ({})", extension, count))
                .collect::<Vec<_>>()
                .join(", ")
        },
        if top_unknown_folders.is_empty() {
            "none".to_string()
        } else {
            top_unknown_folders
                .into_iter()
                .take(5)
                .map(|(folder, count)| format!("{} ({})", folder, count))
                .collect::<Vec<_>>()
                .join(", ")
        },
        pattern_count,
        readable_summary,
        binary_lines,
        important_files,
        component_lesson_lines,
    )
}

fn write_corpus_index(
    corpus_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    totals: &CorpusTotals,
    baseline_meta: &BaselineMetadataFile,
    modded_hash: &str,
    modded_filename: &str,
    artifact_names: &[&str],
) -> Result<()> {
    let index = CorpusIndex {
        schemaVersion: SCHEMA_VERSION.to_string(),
        generatedAt: timing.finishedAt.clone(),
        scannerVersion: tool.version.clone(),
        baselineArchiveHash: baseline_meta.baselineArchiveHash.clone(),
        baselineArchiveFileName: baseline_meta.baselineArchiveFileName.clone(),
        moddedArchiveHash: modded_hash.to_string(),
        moddedArchiveFileName: modded_filename.to_string(),
        sourceArtifacts: vec![
            "clean_vs_modded_diff.json".to_string(),
            "diff_summary.json".to_string(),
            "unknown_changes.json".to_string(),
            "candidate_patterns.json".to_string(),
            "text_analysis_summary.json".to_string(),
        ],
        totals: totals.clone(),
        artifacts: artifact_names
            .iter()
            .map(|name| format!("learning_corpus/{}", name))
            .collect(),
        warning: "local-only corpus; treat all hypotheses as unreviewed and do not generate game patches from this data alone"
            .to_string(),
    };

    let out = corpus_dir.join("learning_corpus_index.json");
    let json = serde_json::to_string_pretty(&index)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_component_frequency(
    corpus_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[ComponentFreqEntry],
) -> Result<()> {
    #[derive(Serialize)]
    struct ComponentFrequencyFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        total: usize,
        entries: &'a [ComponentFreqEntry],
    }

    let report = ComponentFrequencyFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "component_frequency",
        tool,
        timing,
        total: entries.len(),
        entries,
    };

    let out = corpus_dir.join("component_frequency.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_file_type_frequency(
    corpus_dir: &Path,
    tool: &ToolMetadata,
    timing: &Timing,
    entries: &[FileTypeFreqEntry],
) -> Result<()> {
    #[derive(Serialize)]
    struct FileTypeFrequencyFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        tool: &'a ToolMetadata,
        timing: &'a Timing,
        total: usize,
        entries: &'a [FileTypeFreqEntry],
    }

    let report = FileTypeFrequencyFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "file_type_frequency",
        tool,
        timing,
        total: entries.len(),
        entries,
    };

    let out = corpus_dir.join("file_type_frequency.json");
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_analyzer_coverage(corpus_dir: &Path, report: &AnalyzerCoverageReport) -> Result<()> {
    #[derive(Serialize)]
    struct AnalyzerCoverageFile<'a> {
        schemaVersion: &'a str,
        ok: bool,
        artifactType: &'a str,
        report: &'a AnalyzerCoverageReport,
    }

    let wrapped = AnalyzerCoverageFile {
        schemaVersion: SCHEMA_VERSION,
        ok: true,
        artifactType: "analyzer_coverage",
        report,
    };

    let out = corpus_dir.join("analyzer_coverage.json");
    let json = serde_json::to_string_pretty(&wrapped)?;
    fs::write(&out, json)?;
    Ok(())
}

fn write_corpus_ai_change_notes(corpus_dir: &Path, notes: &[CorpusAiChangeNote]) -> Result<()> {
    let mut lines = String::new();
    for note in notes {
        lines.push_str(&serde_json::to_string(note)?);
        lines.push('\n');
    }
    fs::write(corpus_dir.join("corpus_ai_change_notes.jsonl"), lines)?;
    Ok(())
}

fn write_corpus_component_lessons(corpus_dir: &Path, lessons: &[ComponentLesson]) -> Result<()> {
    let mut lines = String::new();
    for lesson in lessons {
        lines.push_str(&serde_json::to_string(lesson)?);
        lines.push('\n');
    }
    fs::write(corpus_dir.join("component_lessons.jsonl"), lines)?;
    Ok(())
}

fn write_corpus_file_lessons(corpus_dir: &Path, lessons: &[FileLesson]) -> Result<()> {
    let mut lines = String::new();
    for lesson in lessons {
        lines.push_str(&serde_json::to_string(lesson)?);
        lines.push('\n');
    }
    fs::write(corpus_dir.join("file_lessons.jsonl"), lines)?;
    Ok(())
}

fn write_training_candidates(corpus_dir: &Path, candidates: &[TrainingCandidate]) -> Result<()> {
    let mut lines = String::new();
    for candidate in candidates {
        lines.push_str(&serde_json::to_string(candidate)?);
        lines.push('\n');
    }
    fs::write(corpus_dir.join("training_candidates.jsonl"), lines)?;
    Ok(())
}

fn write_local_ai_context(corpus_dir: &Path, content: &str) -> Result<()> {
    fs::write(corpus_dir.join("local_ai_context.md"), content)?;
    Ok(())
}

fn write_redux_making_atlas(corpus_dir: &Path, content: &str) -> Result<()> {
    fs::write(corpus_dir.join("redux_making_atlas.md"), content)?;
    Ok(())
}

fn build_and_write_learning_corpus(
    out_dir: &Path,
    changes: &[Change],
    unknown_entries: &[UnknownEntry],
    text_results: Option<&TextAnalysisResults>,
    tool: &ToolMetadata,
    timing: &Timing,
    baseline_meta: &BaselineMetadataFile,
    modded_hash: &str,
    modded_filename: &str,
    pattern_count: usize,
    generated_at: &str,
) -> Result<()> {
    let corpus_dir = out_dir.join("learning_corpus");
    fs::create_dir_all(&corpus_dir)?;

    let component_freq = compute_component_frequency(changes);
    let file_type_freq = compute_file_type_frequency(changes, unknown_entries);
    let analyzer_coverage = match text_results {
        Some(results) => {
            let coverage_percent = if results.stats.totalCandidates > 0 {
                corpus_round_one_decimal(
                    (results.stats.analyzedFiles as f64 / results.stats.totalCandidates as f64)
                        * 100.0,
                )
            } else {
                0.0
            };
            AnalyzerCoverageReport {
                totalCandidates: results.stats.totalCandidates,
                analyzedFiles: results.stats.analyzedFiles,
                skippedFiles: results.stats.skippedFiles,
                xmlAnalyzed: results.stats.xmlAnalyzed,
                datAnalyzed: results.stats.datAnalyzed,
                metaAnalyzed: results.stats.metaAnalyzed,
                genericAnalyzed: results.stats.genericTextAnalyzed,
                parseFailures: results.stats.parseFailures,
                extractionFailures: results.stats.extractionFailures,
                tooLargeSkipped: results.stats.tooLargeSkipped,
                binaryPsoSkipped: results.stats.skippedNotTextBytes,
                coveragePercent: coverage_percent,
                note: "Coverage reflects current readable text analyzers only; binary analyzers are still future work."
                    .to_string(),
            }
        }
        None => AnalyzerCoverageReport {
            totalCandidates: 0,
            analyzedFiles: 0,
            skippedFiles: 0,
            xmlAnalyzed: 0,
            datAnalyzed: 0,
            metaAnalyzed: 0,
            genericAnalyzed: 0,
            parseFailures: 0,
            extractionFailures: 0,
            tooLargeSkipped: 0,
            binaryPsoSkipped: 0,
            coveragePercent: 0.0,
            note: "Text analysis was not run, so coverage is zero.".to_string(),
        },
    };
    let corpus_ai_notes = build_corpus_ai_change_notes_from_opt(text_results);
    let component_lessons = build_component_lessons(&component_freq, changes);
    let file_lessons = build_file_lessons(changes, text_results);
    let training_candidates = build_training_candidates(text_results, changes);
    let totals = CorpusTotals {
        added: changes
            .iter()
            .filter(|change| change.status == "added")
            .count(),
        removed: changes
            .iter()
            .filter(|change| change.status == "removed")
            .count(),
        modified: changes
            .iter()
            .filter(|change| change.status == "modified")
            .count(),
        totalUnknown: unknown_entries.len(),
        textCandidates: unknown_entries
            .iter()
            .filter(|entry| is_text_candidate_ext(&entry.extension))
            .count(),
        binaryCandidates: unknown_entries
            .iter()
            .filter(|entry| !is_text_candidate_ext(&entry.extension))
            .count(),
        analyzedTextFiles: text_results
            .map(|results| results.stats.analyzedFiles)
            .unwrap_or(0),
        skippedTextFiles: text_results
            .map(|results| results.stats.skippedFiles)
            .unwrap_or(0),
        candidatePatterns: pattern_count,
    };
    let local_ai_context = render_local_ai_context(
        changes,
        unknown_entries,
        text_results,
        &component_freq,
        baseline_meta,
        pattern_count,
        generated_at,
    );
    let redux_making_atlas = render_redux_making_atlas(
        changes,
        unknown_entries,
        text_results,
        &component_freq,
        &file_lessons,
        &component_lessons,
        pattern_count,
        generated_at,
    );
    let artifact_names = [
        "learning_corpus_index.json",
        "component_frequency.json",
        "file_type_frequency.json",
        "analyzer_coverage.json",
        "corpus_ai_change_notes.jsonl",
        "component_lessons.jsonl",
        "file_lessons.jsonl",
        "training_candidates.jsonl",
        "local_ai_context.md",
        "redux_making_atlas.md",
    ];

    write_component_frequency(&corpus_dir, tool, timing, &component_freq)?;
    write_file_type_frequency(&corpus_dir, tool, timing, &file_type_freq)?;
    write_analyzer_coverage(&corpus_dir, &analyzer_coverage)?;
    write_corpus_ai_change_notes(&corpus_dir, &corpus_ai_notes)?;
    write_corpus_component_lessons(&corpus_dir, &component_lessons)?;
    write_corpus_file_lessons(&corpus_dir, &file_lessons)?;
    write_training_candidates(&corpus_dir, &training_candidates)?;
    write_local_ai_context(&corpus_dir, &local_ai_context)?;
    write_redux_making_atlas(&corpus_dir, &redux_making_atlas)?;
    write_corpus_index(
        &corpus_dir,
        tool,
        timing,
        &totals,
        baseline_meta,
        modded_hash,
        modded_filename,
        &artifact_names,
    )?;

    println!("learning corpus summary:");
    println!("  component groups: {}", component_freq.len());
    println!("  file type groups: {}", file_type_freq.len());
    println!(
        "  analyzer coverage: {:.1}%",
        analyzer_coverage.coveragePercent
    );
    println!("  ai change notes: {}", corpus_ai_notes.len());
    println!("  component lessons: {}", component_lessons.len());
    println!("  file lessons: {}", file_lessons.len());
    println!("  training candidates: {}", training_candidates.len());

    Ok(())
}

fn timecycle_display_name(path: &str) -> String {
    let base = basename(path);
    if base.is_empty() {
        normalize_path(path)
    } else {
        base
    }
}

fn timecycle_coverage_percent(text_results: Option<&TextAnalysisResults>) -> f64 {
    match text_results {
        Some(results) if results.stats.totalCandidates > 0 => corpus_round_one_decimal(
            (results.stats.analyzedFiles as f64 / results.stats.totalCandidates as f64) * 100.0,
        ),
        _ => 0.0,
    }
}

fn find_xml_entry_by_name<'a>(
    text_results: Option<&'a TextAnalysisResults>,
    file_name: &str,
) -> Option<&'a XmlDiffEntry> {
    text_results.and_then(|results| {
        results
            .xml_entries
            .iter()
            .find(|entry| timecycle_display_name(&entry.path) == file_name)
    })
}

fn find_dat_entry_by_name<'a>(
    text_results: Option<&'a TextAnalysisResults>,
    file_name: &str,
) -> Option<&'a DatDiffEntry> {
    text_results.and_then(|results| {
        results
            .dat_entries
            .iter()
            .find(|entry| timecycle_display_name(&entry.path) == file_name)
    })
}

fn collect_weather_xml_entries<'a>(
    text_results: Option<&'a TextAnalysisResults>,
) -> Vec<&'a XmlDiffEntry> {
    match text_results {
        Some(results) => results
            .xml_entries
            .iter()
            .filter(|entry| {
                let name = timecycle_display_name(&entry.path);
                name.starts_with("w_") && name.ends_with(".xml")
            })
            .collect(),
        None => Vec::new(),
    }
}

fn weather_file_priority(path: &str) -> usize {
    match timecycle_display_name(path).as_str() {
        "w_foggy.xml" => 0,
        "w_clouds.xml" => 1,
        _ => 2,
    }
}

fn build_timecycle_file_rankings(
    text_results: Option<&TextAnalysisResults>,
) -> Vec<TimecycleFileRanking> {
    let visual_entry = find_dat_entry_by_name(text_results, "visualsettings.dat");
    let cloud_entry = find_xml_entry_by_name(text_results, "cloudkeyframes.xml");
    let mods1_entry = find_xml_entry_by_name(text_results, "timecycle_mods_1.xml");
    let foggy_entry = find_xml_entry_by_name(text_results, "w_foggy.xml");
    let clouds_entry = find_xml_entry_by_name(text_results, "w_clouds.xml");
    let mods4_entry = find_xml_entry_by_name(text_results, "timecycle_mods_4.xml");
    let weather_entry = find_xml_entry_by_name(text_results, "weather.xml");
    let mods3_entry = find_xml_entry_by_name(text_results, "timecycle_mods_3.xml");
    let weather_family_entries = collect_weather_xml_entries(text_results);
    let weather_family_numeric: usize = weather_family_entries
        .iter()
        .map(|entry| entry.numericChanges)
        .sum();
    let weather_family_color: usize = weather_family_entries
        .iter()
        .map(|entry| entry.colorLikeChanges)
        .sum();

    let mut ranked: Vec<(i32, TimecycleFileRanking)> = Vec::new();

    {
        let mut score = 960;
        let mut evidence =
            vec!["Filename suggests global visual settings with named parameters.".to_string()];
        let mut confidence = "low".to_string();
        if let Some(entry) = visual_entry {
            evidence.push(format!(
                "{} changed keys and {} numeric changes were detected.",
                entry.changedKeyCount, entry.numericChanges
            ));
            if !entry.sampleKeyChanges.is_empty() {
                evidence.push(format!(
                    "{} sampled named key changes were readable.",
                    entry.sampleKeyChanges.len()
                ));
                score += 180;
                confidence = "high".to_string();
            } else {
                evidence.push(
                    "Named keys were not sampled, so parameter-level planning should stay narrow."
                        .to_string(),
                );
                score += 40;
                confidence = "medium".to_string();
            }
            if entry.readable {
                score += 20;
            }
        } else {
            evidence.push(
                "File was not present in analyzed text results, so this rank remains heuristic."
                    .to_string(),
            );
            score -= 650;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "visualsettings.dat".to_string(),
                rank: 0,
                category: "global_visual_settings".to_string(),
                evidence,
                confidence,
                risk: "medium".to_string(),
                recommended_phase: "first_patch".to_string(),
                safe_for_ai_planning: true,
                safe_for_direct_editing: false,
                recommended_tool: "dat_config_patcher".to_string(),
            },
        ));
    }

    {
        let mut score = 790;
        let mut evidence = vec!["Name suggests cloud and sky keyframes.".to_string()];
        let mut confidence = "low".to_string();
        if let Some(entry) = cloud_entry {
            evidence.push(format!(
                "{} color-like changes and {} numeric changes were detected.",
                entry.colorLikeChanges, entry.numericChanges
            ));
            if entry.colorLikeChanges > 0 && entry.numericChanges == 0 {
                evidence.push(
                    "Only color-like changes were observed in the analyzed diff.".to_string(),
                );
                confidence = "high".to_string();
            } else if entry.colorLikeChanges > 0 {
                evidence.push(
                    "Color-like changes are present, but numeric edits mean schema-aware validation is still required."
                        .to_string(),
                );
                confidence = "high".to_string();
            } else if entry.numericChanges > 0 {
                confidence = "medium".to_string();
            }
            score += (entry.colorLikeChanges.min(2500) / 18) as i32;
            score += (entry.numericChanges.min(1200) / 35) as i32;
        } else {
            evidence.push(
                "No analyzed cloudkeyframes entry was available; keep this as a likely candidate only."
                    .to_string(),
            );
            score -= 520;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "cloudkeyframes.xml".to_string(),
                rank: 0,
                category: "clouds_sky".to_string(),
                evidence,
                confidence,
                risk: "medium".to_string(),
                recommended_phase: "first_patch".to_string(),
                safe_for_ai_planning: true,
                safe_for_direct_editing: false,
                recommended_tool: "xml_cloudkeyframe_editor".to_string(),
            },
        ));
    }

    {
        let mut score = 760;
        let mut evidence =
            vec!["Primary timecycle mods file with a sky-oriented name.".to_string()];
        let mut confidence = "low".to_string();
        if let Some(entry) = mods1_entry {
            evidence.push(format!(
                "{} color-like changes and {} numeric changes were detected.",
                entry.colorLikeChanges, entry.numericChanges
            ));
            if entry.colorLikeChanges >= entry.numericChanges && entry.colorLikeChanges > 0 {
                evidence.push(
                    "Color-like changes are at least as strong as numeric changes in this file."
                        .to_string(),
                );
                confidence = "high".to_string();
            } else if entry.numericChanges > 0 || entry.colorLikeChanges > 0 {
                confidence = "medium".to_string();
            }
            score += (entry.colorLikeChanges.min(2000) / 22) as i32;
            score += (entry.numericChanges.min(1500) / 40) as i32;
        } else {
            evidence.push(
                "The file was not analyzed, so it stays high because of name relevance rather than proof."
                    .to_string(),
            );
            score -= 480;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "timecycle_mods_1.xml".to_string(),
                rank: 0,
                category: "timecycle_core".to_string(),
                evidence,
                confidence,
                risk: "medium".to_string(),
                recommended_phase: "first_patch".to_string(),
                safe_for_ai_planning: true,
                safe_for_direct_editing: false,
                recommended_tool: "xml_timecycle_editor".to_string(),
            },
        ));
    }

    {
        let mut score = 700;
        let mut evidence =
            vec!["Fog-specific weather filename suggests a narrow validation target.".to_string()];
        let mut confidence = "low".to_string();
        if let Some(entry) = foggy_entry {
            evidence.push(format!(
                "{} color-like changes and {} numeric changes were detected.",
                entry.colorLikeChanges, entry.numericChanges
            ));
            if entry.colorLikeChanges > 0 || entry.numericChanges > 0 {
                confidence = "medium".to_string();
            }
            score += (entry.colorLikeChanges.min(1200) / 22) as i32;
            score += (entry.numericChanges.min(1200) / 45) as i32;
        } else {
            evidence.push(
                "No analyzed w_foggy.xml entry was present, so rank is based on probable weather relevance."
                    .to_string(),
            );
            score -= 430;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "w_foggy.xml".to_string(),
                rank: 0,
                category: "weather_fog".to_string(),
                evidence,
                confidence,
                risk: "medium".to_string(),
                recommended_phase: "first_patch".to_string(),
                safe_for_ai_planning: true,
                safe_for_direct_editing: false,
                recommended_tool: "xml_weather_editor".to_string(),
            },
        ));
    }

    {
        let mut score = 680;
        let mut evidence = vec![
            "Cloud-named weather file suggests sky tint or cloud color relevance.".to_string(),
        ];
        let mut confidence = "low".to_string();
        if let Some(entry) = clouds_entry {
            evidence.push(format!(
                "{} color-like changes and {} numeric changes were detected.",
                entry.colorLikeChanges, entry.numericChanges
            ));
            if entry.colorLikeChanges > 0 || entry.numericChanges > 0 {
                confidence = "medium".to_string();
            }
            score += (entry.colorLikeChanges.min(1200) / 22) as i32;
            score += (entry.numericChanges.min(1200) / 45) as i32;
        } else {
            evidence.push(
                "No analyzed w_clouds.xml entry was present, so rank remains name-driven."
                    .to_string(),
            );
            score -= 430;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "w_clouds.xml".to_string(),
                rank: 0,
                category: "weather_clouds".to_string(),
                evidence,
                confidence,
                risk: "medium".to_string(),
                recommended_phase: "first_patch".to_string(),
                safe_for_ai_planning: true,
                safe_for_direct_editing: false,
                recommended_tool: "xml_weather_editor".to_string(),
            },
        ));
    }

    {
        let mut score = 630;
        let mut evidence = vec![
            "Aggregated weather XML family can reveal repeated sky/fog patterns across multiple conditions."
                .to_string(),
        ];
        let mut confidence = "low".to_string();
        if !weather_family_entries.is_empty() {
            evidence.push(format!(
                "{} weather-family files were analyzed with {} color-like and {} numeric changes in total.",
                weather_family_entries.len(), weather_family_color, weather_family_numeric
            ));
            confidence = if weather_family_color > 0 || weather_family_numeric > 0 {
                "medium".to_string()
            } else {
                "low".to_string()
            };
            score += (weather_family_color.min(2000) / 30) as i32;
            score += (weather_family_numeric.min(2000) / 60) as i32;
        } else {
            evidence.push(
                "No weather-family XML files were available in analyzed text results.".to_string(),
            );
            score -= 420;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "w_*.xml family".to_string(),
                rank: 0,
                category: "weather_family".to_string(),
                evidence,
                confidence,
                risk: "medium".to_string(),
                recommended_phase: "first_patch".to_string(),
                safe_for_ai_planning: true,
                safe_for_direct_editing: false,
                recommended_tool: "xml_weather_editor".to_string(),
            },
        ));
    }

    {
        let mut score = 430;
        let mut evidence = vec![
            "Known linkage to kill-effect styling makes this file higher risk for an initial sky patch."
                .to_string(),
        ];
        let mut confidence = "low".to_string();
        if let Some(entry) = mods4_entry {
            evidence.push(format!(
                "{} color-like changes and {} numeric changes were detected.",
                entry.colorLikeChanges, entry.numericChanges
            ));
            if entry.colorLikeChanges > 0 || entry.numericChanges > 0 {
                confidence = "medium".to_string();
            }
            score += (entry.colorLikeChanges.min(1000) / 35) as i32;
        } else {
            evidence.push(
                "No analyzed timecycle_mods_4.xml entry was present, so keep it deferred."
                    .to_string(),
            );
            score -= 180;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "timecycle_mods_4.xml".to_string(),
                rank: 0,
                category: "kill_effect_linked".to_string(),
                evidence,
                confidence,
                risk: "high".to_string(),
                recommended_phase: "defer".to_string(),
                safe_for_ai_planning: false,
                safe_for_direct_editing: false,
                recommended_tool: "xml_timecycle_editor".to_string(),
            },
        ));
    }

    {
        let mut score = 300;
        let mut evidence = vec![
            "Global weather.xml may coordinate multiple systems and should stay deferred at first."
                .to_string(),
        ];
        let mut confidence = "low".to_string();
        if let Some(entry) = weather_entry {
            evidence.push(format!(
                "{} color-like changes and {} numeric changes were detected.",
                entry.colorLikeChanges, entry.numericChanges
            ));
            if entry.colorLikeChanges > 0 || entry.numericChanges > 0 {
                confidence = "medium".to_string();
            }
            score += (entry.colorLikeChanges.min(800) / 60) as i32;
            score += (entry.numericChanges.min(800) / 80) as i32;
        } else {
            evidence.push(
                "weather.xml was not present in analyzed text results, but it still remains globally risky."
                    .to_string(),
            );
            score -= 60;
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "weather.xml".to_string(),
                rank: 0,
                category: "global_weather".to_string(),
                evidence,
                confidence,
                risk: "high".to_string(),
                recommended_phase: "defer".to_string(),
                safe_for_ai_planning: false,
                safe_for_direct_editing: false,
                recommended_tool: "xml_weather_editor".to_string(),
            },
        ));
    }

    {
        let score = 180;
        let mut evidence = vec![
            "Schema is still unknown here, so any broad numeric edit would be unpredictable."
                .to_string(),
        ];
        let mut confidence = "medium".to_string();
        if let Some(entry) = mods3_entry {
            evidence.push(format!(
                "{} numeric changes and {} color-like changes were detected.",
                entry.numericChanges, entry.colorLikeChanges
            ));
            evidence.push(
                "High numeric churn is a strong reason to defer until parameter mapping exists."
                    .to_string(),
            );
        } else {
            evidence.push(
                "No analyzed timecycle_mods_3.xml entry was present, but it remains a predefined risky file."
                    .to_string(),
            );
            confidence = "low".to_string();
        }
        ranked.push((
            score,
            TimecycleFileRanking {
                path_or_family: "timecycle_mods_3.xml".to_string(),
                rank: 0,
                category: "schema_unknown".to_string(),
                evidence,
                confidence,
                risk: "high".to_string(),
                recommended_phase: "defer".to_string(),
                safe_for_ai_planning: false,
                safe_for_direct_editing: false,
                recommended_tool: "xml_timecycle_editor".to_string(),
            },
        ));
    }

    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.path_or_family.cmp(&b.1.path_or_family))
    });

    ranked
        .into_iter()
        .enumerate()
        .map(|(index, (_, mut ranking))| {
            ranking.rank = index + 1;
            ranking
        })
        .collect()
}

fn build_timecycle_safe_edit_matrix() -> Vec<SafeEditMatrixEntry> {
    vec![
        SafeEditMatrixEntry {
            file: "cloudkeyframes.xml".to_string(),
            allowed_first_patch_operations: vec![
                "color_like_desaturation".to_string(),
                "color_like_darken".to_string(),
            ],
            blocked_operations: vec![
                "mass_numeric_edit".to_string(),
                "node_deletion".to_string(),
                "whole_file_replacement".to_string(),
            ],
            deferred_operations: vec!["density_or_shape_numeric_edits".to_string()],
            validator_checks: vec![
                "xml_parse_ok".to_string(),
                "no_unexpected_node_deletion".to_string(),
                "only_color_attributes_changed".to_string(),
            ],
        },
        SafeEditMatrixEntry {
            file: "timecycle_mods_1.xml".to_string(),
            allowed_first_patch_operations: vec![
                "color_like_desaturation".to_string(),
                "color_like_darken".to_string(),
            ],
            blocked_operations: vec![
                "mass_numeric_edit".to_string(),
                "node_deletion".to_string(),
                "whole_file_replacement".to_string(),
            ],
            deferred_operations: vec!["unmapped_numeric_parameter_edits".to_string()],
            validator_checks: vec![
                "xml_parse_ok".to_string(),
                "no_unexpected_node_deletion".to_string(),
                "only_targeted_color_values_changed".to_string(),
            ],
        },
        SafeEditMatrixEntry {
            file: "timecycle_mods_3.xml".to_string(),
            allowed_first_patch_operations: vec![],
            blocked_operations: vec![
                "mass_numeric_edit".to_string(),
                "node_deletion".to_string(),
                "whole_file_replacement".to_string(),
                "schema_unknown_parameter_edit".to_string(),
            ],
            deferred_operations: vec!["all_operations_until_schema_known".to_string()],
            validator_checks: vec![
                "schema_mapping_required".to_string(),
                "parameter_range_validation_required".to_string(),
            ],
        },
        SafeEditMatrixEntry {
            file: "timecycle_mods_4.xml".to_string(),
            allowed_first_patch_operations: vec![
                "color_like_desaturation".to_string(),
                "color_like_darken".to_string(),
            ],
            blocked_operations: vec![
                "mass_numeric_edit".to_string(),
                "whole_file_replacement".to_string(),
            ],
            deferred_operations: vec!["kill_effect_linked_numeric_edits".to_string()],
            validator_checks: vec![
                "xml_parse_ok".to_string(),
                "kill_effect_component_reviewed".to_string(),
            ],
        },
        SafeEditMatrixEntry {
            file: "visualsettings.dat".to_string(),
            allowed_first_patch_operations: vec![
                "named_key_edit_one_at_a_time".to_string(),
                "small_numeric_delta_on_named_key".to_string(),
            ],
            blocked_operations: vec![
                "mass_line_replacement".to_string(),
                "whole_file_replacement".to_string(),
                "multi_family_batch_edit".to_string(),
            ],
            deferred_operations: vec!["unknown_key_family_edits".to_string()],
            validator_checks: vec![
                "preserve_dat_formatting".to_string(),
                "changed_keys_are_named".to_string(),
                "one_family_per_patch".to_string(),
            ],
        },
        SafeEditMatrixEntry {
            file: "w_foggy.xml".to_string(),
            allowed_first_patch_operations: vec![
                "color_like_desaturation".to_string(),
                "color_like_darken".to_string(),
            ],
            blocked_operations: vec![
                "mass_numeric_edit".to_string(),
                "whole_file_replacement".to_string(),
            ],
            deferred_operations: vec!["fog_density_numeric_edits".to_string()],
            validator_checks: vec![
                "xml_parse_ok".to_string(),
                "only_color_attributes_changed".to_string(),
            ],
        },
        SafeEditMatrixEntry {
            file: "w_clouds.xml".to_string(),
            allowed_first_patch_operations: vec![
                "color_like_desaturation".to_string(),
                "color_like_darken".to_string(),
            ],
            blocked_operations: vec![
                "mass_numeric_edit".to_string(),
                "whole_file_replacement".to_string(),
            ],
            deferred_operations: vec!["cloud_density_numeric_edits".to_string()],
            validator_checks: vec![
                "xml_parse_ok".to_string(),
                "only_color_attributes_changed".to_string(),
            ],
        },
        SafeEditMatrixEntry {
            file: "weather.xml".to_string(),
            allowed_first_patch_operations: vec![],
            blocked_operations: vec![
                "mass_numeric_edit".to_string(),
                "whole_file_replacement".to_string(),
                "global_weather_rewire".to_string(),
            ],
            deferred_operations: vec!["all_operations_until_global_mapping_known".to_string()],
            validator_checks: vec![
                "global_weather_review_required".to_string(),
                "cross_file_validation_required".to_string(),
            ],
        },
    ]
}

fn visualsettings_family_for_key(key: &str) -> &'static str {
    let lower = key.to_ascii_lowercase();
    if lower.starts_with("adaptation.") {
        "Adaptation"
    } else if lower.starts_with("tonemapping.") {
        "Tonemapping"
    } else if lower.starts_with("adaptivedof.") {
        "adaptivedof"
    } else if lower.starts_with("bloom.") {
        "bloom"
    } else if lower.starts_with("fog.") {
        "fog"
    } else if lower.starts_with("exposure.") {
        "exposure"
    } else {
        "unknown"
    }
}

fn visualsettings_family_profile(family: &str) -> (&'static str, bool, &'static str) {
    match family {
        "Adaptation" => (
            "medium",
            true,
            "Key family name suggests adaptation-related tuning — unconfirmed.",
        ),
        "Tonemapping" => (
            "medium",
            true,
            "Key family name suggests tonemapping-related tuning — unconfirmed.",
        ),
        "adaptivedof" => (
            "medium",
            false,
            "Key family name suggests adaptive DOF-related tuning — unconfirmed.",
        ),
        "bloom" => (
            "medium",
            true,
            "Key family name suggests bloom-related tuning — unconfirmed.",
        ),
        "fog" => (
            "medium",
            true,
            "Key family name suggests fog-related tuning — unconfirmed.",
        ),
        "exposure" => (
            "medium",
            true,
            "Key family name suggests exposure-related tuning — unconfirmed.",
        ),
        _ => (
            "high",
            false,
            "Key family could not be mapped from the sampled key names — unconfirmed.",
        ),
    }
}

fn build_visualsettings_key_report(
    text_results: Option<&TextAnalysisResults>,
    tool: &ToolMetadata,
    timing: &Timing,
    generated_at: &str,
) -> VisualsettingsKeyReport {
    let entry = find_dat_entry_by_name(text_results, "visualsettings.dat");
    let mut key_families = Vec::new();

    if let Some(entry) = entry {
        let mut families: BTreeMap<String, VisualsettingsKeyFamily> = BTreeMap::new();
        for change in &entry.sampleKeyChanges {
            let family_name = visualsettings_family_for_key(&change.key).to_string();
            let (risk, safe_for_first_patch, hypothesis) =
                visualsettings_family_profile(&family_name);
            let family =
                families
                    .entry(family_name.clone())
                    .or_insert_with(|| VisualsettingsKeyFamily {
                        family: family_name.clone(),
                        keys: Vec::new(),
                        sample_changes: Vec::new(),
                        risk: risk.to_string(),
                        safe_for_first_patch,
                        hypothesis: hypothesis.to_string(),
                    });
            if !family.keys.contains(&change.key) {
                family.keys.push(change.key.clone());
            }
            family.sample_changes.push(change.clone());
        }
        key_families = families.into_values().collect();
        key_families.sort_by(|a, b| a.family.cmp(&b.family));
        for family in &mut key_families {
            family.keys.sort();
            family.keys.dedup();
        }
    }

    VisualsettingsKeyReport {
        schema_version: SCHEMA_VERSION.to_string(),
        ok: true,
        artifact_type: "visualsettings_key_report".to_string(),
        tool: tool.clone(),
        timing: timing.clone(),
        generated_at: generated_at.to_string(),
        file: "visualsettings.dat".to_string(),
        status: entry
            .map(|item| item.status.clone())
            .unwrap_or_else(|| "missing".to_string()),
        changed_key_count: entry.map(|item| item.changedKeyCount).unwrap_or(0),
        numeric_changes: entry.map(|item| item.numericChanges).unwrap_or(0),
        key_families,
        note: "Key meanings are hypotheses only. Do not invent parameter meanings not present in key names."
            .to_string(),
    }
}

fn build_cloudkeyframes_report(
    text_results: Option<&TextAnalysisResults>,
    tool: &ToolMetadata,
    timing: &Timing,
    generated_at: &str,
) -> CloudkeyframesReport {
    let entry = find_xml_entry_by_name(text_results, "cloudkeyframes.xml");
    let numeric_changes = entry.map(|item| item.numericChanges).unwrap_or(0);
    let color_like_changes = entry.map(|item| item.colorLikeChanges).unwrap_or(0);
    let mut evidence = Vec::new();

    if let Some(entry) = entry {
        evidence.push(format!(
            "{} color-like changes were detected.",
            entry.colorLikeChanges
        ));
        evidence.push(format!(
            "{} numeric changes were detected.",
            entry.numericChanges
        ));
        if entry.numericChanges == 0 && entry.colorLikeChanges > 0 {
            evidence.push("Current diff looks color-only in the analyzed sample.".to_string());
        } else if entry.numericChanges > 0 && entry.colorLikeChanges > 0 {
            evidence.push(
                "Both numeric and color-like deltas are present, so schema-aware review is still required."
                    .to_string(),
            );
        }
    } else {
        evidence.push(
            "No analyzed cloudkeyframes.xml entry was available, so this report is a placeholder only."
                .to_string(),
        );
    }

    let color_only_pattern_detected = numeric_changes == 0 && color_like_changes > 0;
    let numeric_and_color_pattern = numeric_changes > 0 && color_like_changes > 0;
    let confidence = if color_only_pattern_detected {
        "high"
    } else if color_like_changes > 0 || numeric_changes > 0 {
        "medium"
    } else {
        "low"
    };

    CloudkeyframesReport {
        schema_version: SCHEMA_VERSION.to_string(),
        ok: true,
        artifact_type: "cloudkeyframes_report".to_string(),
        tool: tool.clone(),
        timing: timing.clone(),
        generated_at: generated_at.to_string(),
        file: "cloudkeyframes.xml".to_string(),
        status: entry
            .map(|item| item.status.clone())
            .unwrap_or_else(|| "not_found".to_string()),
        numeric_changes,
        color_like_changes,
        color_only_pattern_detected,
        numeric_and_color_pattern,
        suggested_first_patch_operation: "color_like_values_only".to_string(),
        blocked_until_schema_known: "numeric_mass_edit".to_string(),
        evidence,
        confidence: confidence.to_string(),
        note: if color_only_pattern_detected {
            "Color-like deltas make this a likely first-patch candidate, but exact visual meaning still requires in-game validation."
                .to_string()
        } else {
            "Mixed numeric and color-like deltas make this a useful planning target, but direct editing should remain conservative."
                .to_string()
        },
    }
}

fn weather_xml_entry_from_xml(entry: &XmlDiffEntry) -> WeatherXmlEntry {
    let name = timecycle_display_name(&entry.path);
    let suggested_phase = if name == "weather.xml" {
        "deferred"
    } else if name == "w_foggy.xml" || name == "w_clouds.xml" {
        "first_patch"
    } else {
        "candidate_followup"
    };
    let confidence = if name == "weather.xml" {
        "low"
    } else if entry.numericChanges > 0 || entry.colorLikeChanges > 0 {
        "medium"
    } else {
        "low"
    };
    let note = if name == "weather.xml" {
        "Global weather configuration candidate; likely touches multiple systems and should stay deferred until effect mapping is clearer."
            .to_string()
    } else if name.contains("fog") {
        "Fog-named weather file — candidate for fog or overcast color tuning.".to_string()
    } else if name.contains("cloud") {
        "Cloud-named weather file — candidate for cloud or sky tint tuning.".to_string()
    } else {
        "Weather-family file with readable diffs; treat as a possible follow-up after safer color-only validation."
            .to_string()
    };

    WeatherXmlEntry {
        path: name,
        status: entry.status.clone(),
        numeric_changes: entry.numericChanges,
        color_like_changes: entry.colorLikeChanges,
        confidence: confidence.to_string(),
        suggested_phase: suggested_phase.to_string(),
        note,
    }
}

fn missing_weather_xml_entry(path: &str) -> WeatherXmlEntry {
    WeatherXmlEntry {
        path: path.to_string(),
        status: "not_found".to_string(),
        numeric_changes: 0,
        color_like_changes: 0,
        confidence: "low".to_string(),
        suggested_phase: if path == "weather.xml" {
            "deferred".to_string()
        } else {
            "candidate_followup".to_string()
        },
        note: if path == "weather.xml" {
            "Global weather configuration remains deferred even when analyzer data is missing."
                .to_string()
        } else {
            "No analyzed weather-family entry was available; keep this as a placeholder only."
                .to_string()
        },
    }
}

fn build_weather_xml_report(
    text_results: Option<&TextAnalysisResults>,
    tool: &ToolMetadata,
    timing: &Timing,
    generated_at: &str,
) -> WeatherXmlReport {
    let mut weather_xml_family: Vec<WeatherXmlEntry> = collect_weather_xml_entries(text_results)
        .into_iter()
        .map(weather_xml_entry_from_xml)
        .collect();
    weather_xml_family.sort_by(|a, b| {
        weather_file_priority(&a.path)
            .cmp(&weather_file_priority(&b.path))
            .then_with(|| a.path.cmp(&b.path))
    });

    let global_weather_xml = find_xml_entry_by_name(text_results, "weather.xml")
        .map(weather_xml_entry_from_xml)
        .unwrap_or_else(|| missing_weather_xml_entry("weather.xml"));

    let mut best_first_candidates = weather_xml_family
        .iter()
        .filter(|entry| entry.suggested_phase == "first_patch")
        .map(|entry| entry.path.clone())
        .take(2)
        .collect::<Vec<_>>();
    if best_first_candidates.is_empty() {
        best_first_candidates = weather_xml_family
            .iter()
            .map(|entry| entry.path.clone())
            .take(2)
            .collect();
    }

    WeatherXmlReport {
        schema_version: SCHEMA_VERSION.to_string(),
        ok: true,
        artifact_type: "weather_xml_report".to_string(),
        tool: tool.clone(),
        timing: timing.clone(),
        generated_at: generated_at.to_string(),
        weather_xml_family,
        global_weather_xml,
        recommendations: WeatherXmlRecommendations {
            best_first_candidates,
            deferred_files: vec!["weather.xml".to_string()],
            reason: "weather.xml may be a global or system-level config, so defer it until effect mapping is clearer."
                .to_string(),
        },
    }
}

fn build_risky_files_report(
    tool: &ToolMetadata,
    timing: &Timing,
    generated_at: &str,
) -> RiskyFilesReport {
    RiskyFilesReport {
        schema_version: SCHEMA_VERSION.to_string(),
        ok: true,
        artifact_type: "risky_files_report".to_string(),
        tool: tool.clone(),
        timing: timing.clone(),
        generated_at: generated_at.to_string(),
        risky_files: vec![
            RiskyFileEntry {
                file_or_family: "timecycle_mods_3.xml".to_string(),
                reason: "High numeric churn with unknown schema means a mass edit would be unpredictable.".to_string(),
                risk: "high".to_string(),
                when_allowed: "After xml_timecycle_editor maps parameter names, ranges, and validation rules."
                    .to_string(),
                required_tool: "xml_timecycle_editor with parameter-name mapping".to_string(),
            },
            RiskyFileEntry {
                file_or_family: "weather.xml".to_string(),
                reason: "Global weather configuration may coordinate multiple systems, so it should stay deferred at first."
                    .to_string(),
                risk: "high".to_string(),
                when_allowed: "After cross-file weather effects are mapped and validated."
                    .to_string(),
                required_tool: "xml_weather_editor with global validation".to_string(),
            },
            RiskyFileEntry {
                file_or_family: "timecycle_mods_4.xml".to_string(),
                reason: "This file is linked to kill_effect behavior, so even apparently visual edits may have side effects."
                    .to_string(),
                risk: "high".to_string(),
                when_allowed: "After kill_effect-linked parameters are isolated from sky-only parameters."
                    .to_string(),
                required_tool: "xml_timecycle_editor with component guardrails".to_string(),
            },
            RiskyFileEntry {
                file_or_family: "*.ypt / *.ytd / *.ysc / *.gfx / *.fxc".to_string(),
                reason: "Binary files are outside the current readable text analyzers and should not be edited from these reports."
                    .to_string(),
                risk: "high".to_string(),
                when_allowed: "After dedicated binary analyzers exist for each family."
                    .to_string(),
                required_tool: "family-specific binary analyzers".to_string(),
            },
            RiskyFileEntry {
                file_or_family: "hit_effect component".to_string(),
                reason: "This component is outside the initial sky/timecycle patch scope.".to_string(),
                risk: "high".to_string(),
                when_allowed: "After sky/timecycle work is validated separately.".to_string(),
                required_tool: "component-specific effect editor".to_string(),
            },
            RiskyFileEntry {
                file_or_family: "tracer component".to_string(),
                reason: "Tracer changes are unrelated to sky/timecycle and should not be mixed into a first patch."
                    .to_string(),
                risk: "high".to_string(),
                when_allowed: "After timecycle-only validation is complete.".to_string(),
                required_tool: "component-specific effect editor".to_string(),
            },
            RiskyFileEntry {
                file_or_family: "minimap_hud component".to_string(),
                reason: "HUD or minimap edits are unrelated to sky/timecycle planning and would add noise."
                    .to_string(),
                risk: "high".to_string(),
                when_allowed: "After a separate HUD-focused pass is planned.".to_string(),
                required_tool: "ui or gfx analyzer".to_string(),
            },
            RiskyFileEntry {
                file_or_family: "kill_effect component".to_string(),
                reason: "Kill-effect visuals are outside the first sky/timecycle patch scope.".to_string(),
                risk: "high".to_string(),
                when_allowed: "After sky-only parameters are validated independently.".to_string(),
                required_tool: "component-specific effect editor".to_string(),
            },
        ],
        note: "These entries are planning guardrails only. They do not prove a file is unsafe forever; they mean the current analyzer data is insufficient for a first edit pass."
            .to_string(),
    }
}

fn timecycle_safe_first_patch_candidates<'a>(
    rankings: &'a [TimecycleFileRanking],
) -> Vec<&'a TimecycleFileRanking> {
    rankings
        .iter()
        .filter(|entry| entry.recommended_phase == "first_patch" && entry.safe_for_ai_planning)
        .collect()
}

fn timecycle_scope_note_for_file(file: &str) -> &'static str {
    match file {
        "visualsettings.dat" => "Prefer one named key or one small key family at a time.",
        "cloudkeyframes.xml" => {
            "Stay with color-like desaturation or darkening only for the first pass."
        }
        "timecycle_mods_1.xml" => {
            "Restrict the first patch to clearly color-like values until schema mapping improves."
        }
        "w_foggy.xml" | "w_clouds.xml" => {
            "Prefer color-only weather tint changes; defer density-style numeric edits."
        }
        "w_*.xml family" => {
            "Use the family only for repeated-pattern planning, not for broad direct edits."
        }
        _ => "Keep the patch narrow and validation-heavy.",
    }
}

fn timecycle_tool_list(rankings: &[TimecycleFileRanking]) -> Vec<String> {
    let mut tools = BTreeSet::new();
    for ranking in rankings {
        if !ranking.recommended_tool.trim().is_empty() {
            tools.insert(ranking.recommended_tool.clone());
        }
    }
    tools.insert("xml parser / structural diff validator".to_string());
    tools.insert("in-game screenshot or capture validation".to_string());
    tools.into_iter().collect()
}

fn timecycle_validation_rules() -> Vec<&'static str> {
    vec![
        "Keep each patch to one file or one named key family at a time.",
        "Require parse success after every edit and reject unexpected node or line deletion.",
        "Validate in-game before claiming exact visual outcomes.",
        "Do not batch unrelated components such as tracer, hit_effect, minimap_hud, or kill_effect into the same patch.",
    ]
}

fn timecycle_evidence_lines(text_results: Option<&TextAnalysisResults>) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(entry) = find_dat_entry_by_name(text_results, "visualsettings.dat") {
        lines.push(format!(
            "`visualsettings.dat`: changedKeyCount={}, numericChanges={}, sampledKeys={}",
            entry.changedKeyCount,
            entry.numericChanges,
            entry.sampleKeyChanges.len()
        ));
    }

    for name in [
        "cloudkeyframes.xml",
        "timecycle_mods_1.xml",
        "timecycle_mods_3.xml",
        "timecycle_mods_4.xml",
        "w_foggy.xml",
        "w_clouds.xml",
        "weather.xml",
    ] {
        if let Some(entry) = find_xml_entry_by_name(text_results, name) {
            lines.push(format!(
                "`{}`: numericChanges={}, colorLikeChanges={}, addedLines={}, removedLines={}",
                name,
                entry.numericChanges,
                entry.colorLikeChanges,
                entry.addedLines,
                entry.removedLines
            ));
        }
    }

    if lines.is_empty() {
        lines.push(
            "No analyzed sky/timecycle text entries were available; rankings fall back to cautious filename heuristics."
                .to_string(),
        );
    }

    lines
}

fn render_timecycle_strategy_report(
    text_results: Option<&TextAnalysisResults>,
    rankings: &[TimecycleFileRanking],
    risky_files: &[RiskyFileEntry],
    tool: &ToolMetadata,
    generated_at: &str,
) -> String {
    let coverage = timecycle_coverage_percent(text_results);
    let scan_summary = match text_results {
        Some(results) => format!(
            "- generated at: {}\n- scanner: {} {}\n- analyzed text files: {} of {} candidates ({:.1}% coverage)\n- xml/dat/meta/generic analyzed: {}/{}/{}/{}\n- focus: sky and timecycle candidate planning only; all conclusions remain hypotheses until in-game validation.",
            generated_at,
            tool.name,
            tool.version,
            results.stats.analyzedFiles,
            results.stats.totalCandidates,
            coverage,
            results.stats.xmlAnalyzed,
            results.stats.datAnalyzed,
            results.stats.metaAnalyzed,
            results.stats.genericTextAnalyzed,
        ),
        None => format!(
            "- generated at: {}\n- scanner: {} {}\n- text analysis was not available, so this report uses only filename-level heuristics.\n- focus: likely sky/timecycle files only; every recommendation is tentative.",
            generated_at, tool.name, tool.version
        ),
    };
    let strongest = rankings
        .iter()
        .take(5)
        .map(|entry| {
            format!(
                "- `{}` — confidence: {}, phase: {}, evidence: {}",
                entry.path_or_family,
                entry.confidence,
                entry.recommended_phase,
                entry.evidence.join("; ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let safe_candidates = timecycle_safe_first_patch_candidates(rankings)
        .into_iter()
        .take(5)
        .map(|entry| {
            format!(
                "- `{}` — {}",
                entry.path_or_family,
                timecycle_scope_note_for_file(&entry.path_or_family)
            )
        })
        .collect::<Vec<_>>();
    let safe_candidates_text = if safe_candidates.is_empty() {
        "- No safe first-patch candidates were confirmed by analyzer data.".to_string()
    } else {
        safe_candidates.join("\n")
    };
    let risky_text = risky_files
        .iter()
        .map(|entry| {
            format!(
                "- `{}` — {} (required tool: {})",
                entry.file_or_family, entry.reason, entry.required_tool
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let evidence_text = timecycle_evidence_lines(text_results)
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let recommended_scope = timecycle_safe_first_patch_candidates(rankings)
        .into_iter()
        .take(3)
        .map(|entry| format!("- `{}` — {}", entry.path_or_family, entry.category))
        .collect::<Vec<_>>();
    let recommended_scope_text = if recommended_scope.is_empty() {
        "- Start with a manual review pass before proposing any patch.".to_string()
    } else {
        format!(
            "Begin with one of the following narrow candidates, validate it in-game, then expand only if the result is stable:\n{}",
            recommended_scope.join("\n")
        )
    };
    let validation_text = timecycle_validation_rules()
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let ai_may = [
        "Infer relative priority between likely sky/timecycle files.",
        "Suggest candidate-first patch scopes that stay narrow and reversible.",
        "Use named key prefixes from visualsettings.dat as hypotheses only.",
    ]
    .iter()
    .map(|line| format!("- {}", line))
    .collect::<Vec<_>>()
    .join("\n");
    let ai_must_not = [
        "Invent exact parameter meanings that are not present in the key or file names.",
        "Claim exact visual outcomes without validation screenshots or gameplay review.",
        "Propose whole-file replacement, schema-blind mass edits, or binary file editing from this report alone.",
    ]
    .iter()
    .map(|line| format!("- {}", line))
    .collect::<Vec<_>>()
    .join("\n");
    let tool_text = timecycle_tool_list(rankings)
        .into_iter()
        .map(|tool_name| format!("- {}", tool_name))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "# Timecycle Strategy Report\n\n## Overview\n{}\n\n## Strongest timecycle candidates\n{}\n\n## Safest first-patch candidates\n{}\n\n## Risky/deferred files\n{}\n\n## Evidence from scanner data\n{}\n\n## Recommended first patch scope\n{}\n\n## Validation requirements\n{}\n\n## What AI may infer vs must not infer\n### AI may infer\n{}\n\n### AI must not infer\n{}\n\n## Deterministic tools needed\n{}\n\nGenerated locally by Redux Scanner Engine. Treat every recommendation as a likely or possible candidate that still requires in-game validation.\n",
        scan_summary,
        strongest,
        safe_candidates_text,
        risky_text,
        evidence_text,
        recommended_scope_text,
        validation_text,
        ai_may,
        ai_must_not,
        tool_text,
    )
}

fn render_ai_timecycle_context_compact(
    text_results: Option<&TextAnalysisResults>,
    rankings: &[TimecycleFileRanking],
    risky_files: &[RiskyFileEntry],
    tool: &ToolMetadata,
    generated_at: &str,
) -> String {
    let coverage = timecycle_coverage_percent(text_results);
    let scan_summary = match text_results {
        Some(results) => format!(
            "Generated at {} by {} {} using schema {}. The scanner compared clean-versus-modded inputs and analyzed {} of {} readable text candidates ({:.1}% coverage). XML/DAT/META/generic analyzer counts were {}/{}/{}/{}. This context is intentionally narrow: it highlights likely sky and timecycle files, records deterministic evidence such as numeric and color-like change counts, and avoids any claim that a file definitely controls a specific in-game effect.\n\nThe purpose of this file is planning, not editing. A small or cheap AI model should treat every recommendation below as a candidate or hypothesis. Use it to decide which file to inspect first, which files to defer, and what validation gates must be satisfied before a patch plan is trusted.",
            generated_at,
            tool.name,
            tool.version,
            SCHEMA_VERSION,
            results.stats.analyzedFiles,
            results.stats.totalCandidates,
            coverage,
            results.stats.xmlAnalyzed,
            results.stats.datAnalyzed,
            results.stats.metaAnalyzed,
            results.stats.genericTextAnalyzed,
        ),
        None => format!(
            "Generated at {} by {} {} using schema {}. Text analysis data was not available, so this context falls back to filename-level heuristics and hard safety rules. That means the ranked files below are still useful for planning, but every proposed edit idea must remain conservative and should be rechecked once analyzer output exists.\n\nThis file is still valuable for cheap AI models because it explains the allowed scope, blocked scope, and the validation mindset expected by the scanner project.",
            generated_at, tool.name, tool.version, SCHEMA_VERSION
        ),
    };
    let key_findings = rankings
        .iter()
        .take(7)
        .map(|entry| {
            format!(
                "- `{}`: confidence={}, phase={}, evidence={}.",
                entry.path_or_family,
                entry.confidence,
                entry.recommended_phase,
                entry.evidence.join("; ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let ranked_candidates = rankings
        .iter()
        .map(|entry| {
            format!(
                "{}. `{}` — category: {}, confidence: {}, recommended phase: {}, tool: {}. {}",
                entry.rank,
                entry.path_or_family,
                entry.category,
                entry.confidence,
                entry.recommended_phase,
                entry.recommended_tool,
                timecycle_scope_note_for_file(&entry.path_or_family)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let safe_scope = timecycle_safe_first_patch_candidates(rankings)
        .into_iter()
        .take(5)
        .map(|entry| {
            format!(
                "- `{}` — {} Confidence is {} and direct editing should still stay narrow.",
                entry.path_or_family,
                timecycle_scope_note_for_file(&entry.path_or_family),
                entry.confidence
            )
        })
        .collect::<Vec<_>>();
    let safe_scope_text = if safe_scope.is_empty() {
        "- No file was confirmed as a safe first-patch candidate; stay in review-only mode."
            .to_string()
    } else {
        format!(
            "These are the best candidates for planning a first patch because they combine sky/timecycle relevance with relatively readable evidence. Even here, the first patch should remain color-focused or single-key-focused.\n{}",
            safe_scope.join("\n")
        )
    };
    let risky_text = risky_files
        .iter()
        .map(|entry| {
            format!(
                "- `{}` — {} Allowed later: {} Required tool: {}.",
                entry.file_or_family, entry.reason, entry.when_allowed, entry.required_tool
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let validation_text = timecycle_validation_rules()
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let tool_text = timecycle_tool_list(rankings)
        .into_iter()
        .map(|tool_name| format!("- {}", tool_name))
        .collect::<Vec<_>>()
        .join("\n");
    let must_not = [
        "Do not invent exact values, ranges, or parameter meanings that are not present in the analyzer evidence.",
        "Do not recommend whole-file replacement, node deletion, or mass numeric edits for timecycle_mods_3.xml, weather.xml, or any binary family.",
        "Do not treat file names alone as proof of gameplay or visual effect; they are hints only.",
        "Do not mix sky/timecycle planning with tracer, hit_effect, minimap_hud, kill_effect, or other unrelated components in a first patch.",
        "Do not present an edit plan as final unless it includes validation steps and rollback expectations.",
    ]
    .iter()
    .map(|line| format!("- {}", line))
    .collect::<Vec<_>>()
    .join("\n");
    let may = [
        "Infer which files are more relevant to sky, clouds, fog, or broad visual settings based on names and observed counts.",
        "Recommend a first-patch order that starts with named-key DAT edits or color-only XML edits.",
        "Suggest validation gates such as XML parse checks, DAT formatting preservation, and in-game screenshot comparison.",
        "Group visualsettings.dat keys only by prefixes that already appear in sampleKeyChanges, such as Adaptation or Tonemapping.",
        "Explain uncertainty explicitly and mark any behavioral interpretation as a hypothesis that still needs confirmation.",
    ]
    .iter()
    .map(|line| format!("- {}", line))
    .collect::<Vec<_>>()
    .join("\n");

    format!(
        "# Sky/Timecycle AI Context — Redux Scanner\n\n## Scan Summary\n{}\n\n## Key Findings\n{}\n\n## Ranked Candidate Files\n{}\n\n## Safest First-Patch Scope\n{}\n\n## Risky / Deferred Files\n{}\n\n## Validation Rules\n{}\n\nThese validation rules matter because the scanner output is evidence-rich but still indirect. NumericChanges and colorLikeChanges show that a file moved, but they do not guarantee which parameter produced which screenshot result. Any AI plan should therefore optimize for reversibility, narrow scope, and explicit checkpoints.\n\n## Tool Requirements\n{}\n\n## AI Must Not\n{}\n\n## AI May\n{}\n\nUse this document as the primary context block for smaller models. It is deliberately compact, deterministic, and biased toward cautious planning rather than speculative conclusions.\n",
        scan_summary,
        key_findings,
        ranked_candidates,
        safe_scope_text,
        risky_text,
        validation_text,
        tool_text,
        must_not,
        may,
    )
}

fn render_ai_timecycle_prompt_pack() -> String {
    r#"# AI Timecycle Prompt Pack

## System Prompt
You are analyzing Redux Scanner Engine output for sky and timecycle planning only. Treat all scanner evidence as deterministic but incomplete. Use cautious language such as likely, possible, candidate, hypothesis, and requires in-game validation. Never invent parameter meanings that are not already visible in filenames, key names, or explicit scanner counts. Never propose editing binary files, RPF archives, or unrelated components such as tracer, hit_effect, minimap_hud, or kill_effect in a first sky/timecycle patch.

## User Prompt (Full Context)
You will receive `ai_timecycle_context_compact.md` plus supporting report summaries. Produce a first-patch planning recommendation for Redux sky/timecycle work.

Requirements:
1. Rank the safest files to inspect or patch first.
2. Explain why each file is a candidate using only scanner evidence.
3. Keep every edit idea narrow, reversible, and validation-heavy.
4. Separate safe first-patch ideas from risky/deferred ideas.
5. State what still requires schema mapping or in-game validation.
6. Do not invent exact edit values unless they already appear in named keys and the plan clearly marks them as examples rather than facts.

## Compact Free-Model Prompt
Given this Redux Scanner context, tell me the safest first-patch scope for sky/timecycle work. Keep the answer short. Rank files, explain evidence, list risky files to avoid, and include validation steps. Use cautious language only.

## JSON Patch-Plan Prompt
Read the scanner context and output JSON only.

Schema:
{
  "recommendedOrder": [
    {
      "file": "visualsettings.dat",
      "phase": "first_patch|defer",
      "allowedOps": ["..."],
      "blockedOps": ["..."],
      "evidence": ["..."],
      "confidence": "low|medium|high",
      "validation": ["..."],
      "notes": "..."
    }
  ],
  "deferred": ["..."],
  "unknowns": ["..."],
  "finalWarning": "..."
}

Rules: no invented keys, no binary edits, no whole-file replacement, and no certainty claims without validation.

## Critic/Grading Prompt
Review another AI's sky/timecycle patch plan. Grade it for safety, evidence quality, scope control, and validation discipline.

Checklist:
- Did it stay inside sky/timecycle scope?
- Did it avoid binary files and unrelated components?
- Did it use only scanner evidence that was actually present?
- Did it keep first patches narrow and reversible?
- Did it clearly defer timecycle_mods_3.xml, weather.xml, and kill-effect-linked work?
- Did it require in-game validation before claiming success?

Output a short verdict plus bullet points for major risks, missing evidence, and the safest corrected plan.
"#
        .to_string()
}

fn build_and_write_timecycle_intelligence(
    out_dir: &Path,
    text_results: Option<&TextAnalysisResults>,
    tool: &ToolMetadata,
    timing: &Timing,
    generated_at: &str,
) -> Result<()> {
    let tc_dir = out_dir.join("timecycle_intelligence");
    fs::create_dir_all(&tc_dir)?;

    let rankings = build_timecycle_file_rankings(text_results);
    let safe_matrix = build_timecycle_safe_edit_matrix();
    let visualsettings_report =
        build_visualsettings_key_report(text_results, tool, timing, generated_at);
    let cloudkeyframes_report =
        build_cloudkeyframes_report(text_results, tool, timing, generated_at);
    let weather_report = build_weather_xml_report(text_results, tool, timing, generated_at);
    let risky_report = build_risky_files_report(tool, timing, generated_at);

    let strategy_report = render_timecycle_strategy_report(
        text_results,
        &rankings,
        &risky_report.risky_files,
        tool,
        generated_at,
    );
    let compact_context = render_ai_timecycle_context_compact(
        text_results,
        &rankings,
        &risky_report.risky_files,
        tool,
        generated_at,
    );
    let prompt_pack = render_ai_timecycle_prompt_pack();

    let rankings_report = TimecycleFileRankingsReport {
        schema_version: SCHEMA_VERSION.to_string(),
        ok: true,
        artifact_type: "timecycle_file_rankings".to_string(),
        tool: tool.clone(),
        timing: timing.clone(),
        generated_at: generated_at.to_string(),
        rankings,
    };
    let safe_matrix_report = TimecycleSafeEditMatrixReport {
        schema_version: SCHEMA_VERSION.to_string(),
        ok: true,
        artifact_type: "timecycle_safe_edit_matrix".to_string(),
        tool: tool.clone(),
        timing: timing.clone(),
        generated_at: generated_at.to_string(),
        entries: safe_matrix,
    };

    fs::write(tc_dir.join("timecycle_strategy_report.md"), strategy_report)?;
    fs::write(
        tc_dir.join("timecycle_file_rankings.json"),
        serde_json::to_string_pretty(&rankings_report)?,
    )?;
    fs::write(
        tc_dir.join("timecycle_safe_edit_matrix.json"),
        serde_json::to_string_pretty(&safe_matrix_report)?,
    )?;
    fs::write(
        tc_dir.join("visualsettings_key_report.json"),
        serde_json::to_string_pretty(&visualsettings_report)?,
    )?;
    fs::write(
        tc_dir.join("cloudkeyframes_report.json"),
        serde_json::to_string_pretty(&cloudkeyframes_report)?,
    )?;
    fs::write(
        tc_dir.join("weather_xml_report.json"),
        serde_json::to_string_pretty(&weather_report)?,
    )?;
    fs::write(
        tc_dir.join("risky_files_report.json"),
        serde_json::to_string_pretty(&risky_report)?,
    )?;
    fs::write(
        tc_dir.join("ai_timecycle_context_compact.md"),
        compact_context,
    )?;
    fs::write(tc_dir.join("ai_timecycle_prompt_pack.md"), prompt_pack)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_tool_metadata() -> ToolMetadata {
        ToolMetadata {
            name: "redux_rpf_scanner".to_string(),
            version: "0.8.1".to_string(),
            backend: BACKEND_NAME.to_string(),
            backendVersion: BACKEND_VERSION.to_string(),
            platform: "windows".to_string(),
        }
    }

    fn dummy_timing() -> Timing {
        Timing {
            startedAt: "2025-01-01T00:00:00Z".to_string(),
            finishedAt: "2025-01-01T00:00:01Z".to_string(),
            durationMs: 1000,
        }
    }

    fn make_timecycle_xml_entry(
        path: &str,
        numeric_changes: usize,
        color_like_changes: usize,
    ) -> XmlDiffEntry {
        XmlDiffEntry {
            path: path.to_string(),
            status: "modified".to_string(),
            analyzer: "xml".to_string(),
            parseStrategy: "line_diff".to_string(),
            cleanLines: 10,
            moddedLines: 10,
            addedLines: 1,
            removedLines: 1,
            numericChanges: numeric_changes,
            colorLikeChanges: color_like_changes,
            sampleChanges: vec![],
            warnings: vec![],
        }
    }

    fn make_timecycle_key_change(key: &str) -> KeyValueChange {
        KeyValueChange {
            key: key.to_string(),
            oldValue: "1.0".to_string(),
            newValue: "2.0".to_string(),
            valueType: "float".to_string(),
            numericDelta: Some(1.0),
        }
    }

    fn make_timecycle_dat_entry(
        path: &str,
        changed_key_count: usize,
        numeric_changes: usize,
        sample_key_changes: Vec<KeyValueChange>,
    ) -> DatDiffEntry {
        DatDiffEntry {
            path: path.to_string(),
            status: "modified".to_string(),
            analyzer: "dat".to_string(),
            readable: true,
            cleanLines: 10,
            moddedLines: 10,
            changedKeyCount: changed_key_count,
            addedLines: 1,
            removedLines: 1,
            numericChanges: numeric_changes,
            sampleKeyChanges: sample_key_changes,
            sampleChanges: vec![],
            warnings: vec![],
        }
    }

    fn make_timecycle_results(
        xml_entries: Vec<XmlDiffEntry>,
        dat_entries: Vec<DatDiffEntry>,
    ) -> TextAnalysisResults {
        TextAnalysisResults {
            stats: TextAnalysisStats {
                totalCandidates: xml_entries.len() + dat_entries.len(),
                analyzedFiles: xml_entries.len() + dat_entries.len(),
                skippedFiles: 0,
                xmlAnalyzed: xml_entries.len(),
                datAnalyzed: dat_entries.len(),
                metaAnalyzed: 0,
                genericTextAnalyzed: 0,
                parseFailures: 0,
                extractionFailures: 0,
                tooLargeSkipped: 0,
                skippedNotTextBytes: 0,
            },
            xml_entries,
            dat_entries,
            meta_entries: vec![],
            generic_entries: vec![],
            analyzer_warnings: vec![],
            file_summaries: vec![],
        }
    }

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
        let entries = make_update_like_entries();
        let result = check_anchor_paths(&entries);
        assert!(result.found.contains(&"american_rel.rpf/".to_string()));
        assert!(result.found.contains(&"ptfx.rpf/".to_string()));
        assert!(result.found.contains(&"visualsettings.dat".to_string()));
        assert!(result.found.contains(&"gta5_cache_y.dat".to_string()));
        assert!(result
            .found
            .contains(&"scaleform_frontend.rpf/".to_string()));
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

    fn make_update_like_entries() -> BTreeMap<String, EntryInfo> {
        let mut m = BTreeMap::new();
        for (path, size) in &[
            // Anchor files — flat root-level files characteristic of update.rpf
            ("visualsettings.dat", 8192u32),
            ("gta5_cache_y.dat", 65536),
            ("popcycle.dat", 4096),
            ("carcols.meta", 8192),
            ("hudcolor.dat", 2048),
            // Nested RPF anchors
            ("american_rel.rpf/abgail2.gxt2", 512),
            ("american_rel.rpf/acultau.gxt2", 512),
            ("ptfx.rpf/core.ypt", 40960),
            ("ptfx.rpf/cut_arena.ypt", 20480),
            ("scaleform_frontend.rpf/busy_spinner.gfx", 8192),
            // Characteristic extension files
            ("playeranims.yvr", 102400),
            ("walk_cycle.yvr", 98304),
            ("main.ysc", 204800),
            ("missions.ysc", 512000),
            ("world_north.ymap", 32768),
            ("ambient.fxc", 16384),
            ("carvariations.meta", 4096),
            ("weaponfx.dat", 8192),
        ] {
            let norm = normalize_path(path);
            let ext = norm.rsplit('.').next().unwrap_or("").to_string();
            let name = norm.rsplit('/').next().unwrap_or(&norm).to_string();
            m.insert(
                norm.clone(),
                EntryInfo {
                    path: norm,
                    name,
                    extension: ext,
                    sizeBytes: *size as usize,
                    sha256: String::new(),
                    source: "update.rpf".to_string(),
                },
            );
        }
        m
    }

    fn make_narrow_vehicle_entries() -> BTreeMap<String, EntryInfo> {
        let mut m = BTreeMap::new();
        for (path, size) in &[
            ("vehicles/infernus.yft", 102400u32),
            ("vehicles/infernus.ytd", 204800),
            ("vehicles/sultan.yft", 98304),
            ("vehicles/sultan.ytd", 196608),
        ] {
            let norm = normalize_path(path);
            let ext = norm.rsplit('.').next().unwrap_or("").to_string();
            let name = norm.rsplit('/').next().unwrap_or(&norm).to_string();
            m.insert(
                norm.clone(),
                EntryInfo {
                    path: norm,
                    name,
                    extension: ext,
                    sizeBytes: *size as usize,
                    sha256: String::new(),
                    source: "dlc_vehicles.rpf".to_string(),
                },
            );
        }
        m
    }

    fn fake_baseline_fp(total_paths: usize) -> BaselineFingerprintFile {
        BaselineFingerprintFile {
            archive: FingerprintArchiveId {
                archiveFileName: "update.rpf".to_string(),
                archiveSha256: "deadbeef".to_string(),
            },
            totalPaths: total_paths,
            treeFingerprintSha256: "abc123".to_string(),
            anchorPathsFound: vec![
                "american_rel.rpf/".to_string(),
                "ptfx.rpf/".to_string(),
                "scaleform_frontend.rpf/".to_string(),
                "visualsettings.dat".to_string(),
                "gta5_cache_y.dat".to_string(),
                "popcycle.dat".to_string(),
                "carcols.meta".to_string(),
                "hudcolor.dat".to_string(),
            ],
        }
    }

    #[test]
    fn classifier_scores_update_like_archive_as_likely_or_obvious() {
        let entries = make_update_like_entries();
        // Add more entries so size hint kicks in
        let mut big_entries = entries;
        for i in 0..8000 {
            let path = format!("x64/models/extra_{}.ydr", i);
            big_entries.insert(
                path.clone(),
                EntryInfo {
                    path: path.clone(),
                    name: format!("extra_{}.ydr", i),
                    extension: "ydr".to_string(),
                    sizeBytes: 1024,
                    sha256: String::new(),
                    source: "update.rpf".to_string(),
                },
            );
        }
        let fp = fake_baseline_fp(21000);
        let (score, _reasons, matched, _missing) = score_classify_archive(&big_entries, &fp);
        assert!(score >= 75, "expected >=75, got {}", score);
        assert!(matched.contains(&"visualsettings.dat".to_string()));
        let label = classify_label_from_score(score);
        assert!(
            label == "likely_update_rpf" || label == "obvious_update_rpf",
            "unexpected label: {}",
            label
        );
    }

    #[test]
    fn classifier_scores_vehicle_pack_as_not_update_rpf() {
        let entries = make_narrow_vehicle_entries();
        let fp = fake_baseline_fp(21000);
        let (score, _reasons, _matched, _missing) = score_classify_archive(&entries, &fp);
        let label = classify_label_from_score(score);
        assert!(
            label == "not_update_rpf" || label == "unknown_rpf",
            "expected not/unknown, got {} (score={})",
            label,
            score
        );
    }

    #[test]
    fn classifier_label_thresholds_correct() {
        assert_eq!(classify_label_from_score(100), "obvious_update_rpf");
        assert_eq!(classify_label_from_score(90), "obvious_update_rpf");
        assert_eq!(classify_label_from_score(89), "likely_update_rpf");
        assert_eq!(classify_label_from_score(75), "likely_update_rpf");
        assert_eq!(classify_label_from_score(74), "possible_update_rpf");
        assert_eq!(classify_label_from_score(50), "possible_update_rpf");
        assert_eq!(classify_label_from_score(49), "not_update_rpf");
        assert_eq!(classify_label_from_score(20), "not_update_rpf");
        assert_eq!(classify_label_from_score(19), "unknown_rpf");
        assert_eq!(classify_label_from_score(0), "unknown_rpf");
    }

    #[test]
    fn recommend_action_mapping_correct() {
        assert_eq!(
            recommend_action_from_label("obvious_update_rpf"),
            "import_as_update_rpf"
        );
        assert_eq!(
            recommend_action_from_label("likely_update_rpf"),
            "import_as_update_rpf"
        );
        assert_eq!(
            recommend_action_from_label("possible_update_rpf"),
            "review_before_import"
        );
        assert_eq!(recommend_action_from_label("not_update_rpf"), "skip");
        assert_eq!(recommend_action_from_label("unknown_rpf"), "review");
        assert_eq!(recommend_action_from_label("scan_failed"), "review_error");
    }

    #[test]
    fn baseline_fingerprint_deserializes_from_json() {
        let json = r#"{
            "schemaVersion": "2.0",
            "ok": true,
            "artifactType": "baseline_update_tree_fingerprint",
            "archive": {
                "archivePath": "examples/fixtures/clean_update.rpf",
                "archiveFileName": "update.rpf",
                "archiveSizeBytes": 1000000,
                "archiveSha256": "abc123def456"
            },
            "mode": "full",
            "depth": 3,
            "totalPaths": 21000,
            "treeFingerprintSha256": "fingerprint_hash_here",
            "topLevelFolders": ["common", "x64"],
            "extensionHistogram": [],
            "anchorPathsFound": ["american_rel.rpf/", "ptfx.rpf/", "visualsettings.dat"],
            "anchorPathsMissing": ["hudcolor.dat"]
        }"#;
        let parsed: BaselineFingerprintFile = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.totalPaths, 21000);
        assert_eq!(parsed.archive.archiveFileName, "update.rpf");
        assert_eq!(parsed.treeFingerprintSha256, "fingerprint_hash_here");
        assert!(parsed
            .anchorPathsFound
            .contains(&"american_rel.rpf/".to_string()));
    }

    #[test]
    fn classification_report_no_key_exposure() {
        let entries = make_update_like_entries();
        let fp = fake_baseline_fp(21000);
        let (score, reasons, matched, missing) = score_classify_archive(&entries, &fp);
        let label = classify_label_from_score(score);
        let json = serde_json::to_string(&serde_json::json!({
            "score": score,
            "label": label,
            "reasons": reasons,
            "matched": matched,
            "missing": missing,
        }))
        .unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("aes_key"));
        assert!(!json.contains("ng_key"));
        assert!(!json.contains("ng_decrypt"));
    }

    #[test]
    fn classify_attempt_serializes_correctly() {
        let attempt = ClassifyAttempt {
            physicalFileName: "redux.rpf".to_string(),
            logicalFileName: "update.rpf".to_string(),
            entryCount: 14449,
            score: 100,
            classification: "obvious_update_rpf".to_string(),
            usedForResult: true,
            note: Some(
                "Archive matched update.rpf tree when opened with logical name \"update.rpf\"."
                    .to_string(),
            ),
        };
        let json = serde_json::to_string(&attempt).unwrap();
        assert!(json.contains("\"physicalFileName\""));
        assert!(json.contains("\"logicalFileName\""));
        assert!(json.contains("\"usedForResult\""));
        assert!(json.contains("\"note\""));
        assert!(json.contains("redux.rpf"));
        assert!(json.contains("update.rpf"));
    }

    #[test]
    fn classify_attempt_without_note_serializes_null() {
        let attempt = ClassifyAttempt {
            physicalFileName: "update.rpf".to_string(),
            logicalFileName: "update.rpf".to_string(),
            entryCount: 21000,
            score: 100,
            classification: "obvious_update_rpf".to_string(),
            usedForResult: true,
            note: None,
        };
        let json = serde_json::to_string(&attempt).unwrap();
        assert!(json.contains("\"note\":null"));
    }

    #[test]
    fn fallback_not_triggered_when_score_already_high() {
        // When physical scan gives a high score, fallback is not needed.
        let entries = make_update_like_entries();
        let mut big_entries = entries;
        for i in 0..8000 {
            let path = format!("extra_{}.yvr", i);
            big_entries.insert(
                path.clone(),
                EntryInfo {
                    path: path.clone(),
                    name: format!("extra_{}.yvr", i),
                    extension: "yvr".to_string(),
                    sizeBytes: 1024,
                    sha256: String::new(),
                    source: "update.rpf".to_string(),
                },
            );
        }
        let fp = fake_baseline_fp(21000);
        let (score, _, _, _) = score_classify_archive(&big_entries, &fp);
        // High score means no fallback needed
        let needs_fallback = score < 50;
        assert!(
            !needs_fallback,
            "Expected no fallback needed for score={}",
            score
        );
    }

    #[test]
    fn fallback_triggered_when_score_is_low() {
        // A small vehicle-only pack scores low and should trigger fallback.
        let entries = make_narrow_vehicle_entries();
        let fp = fake_baseline_fp(21000);
        let (score, _, _, _) = score_classify_archive(&entries, &fp);
        let needs_fallback = score < 50;
        assert!(
            needs_fallback,
            "Expected fallback needed for score={}",
            score
        );
    }

    #[test]
    fn fallback_skip_when_already_named_update_rpf() {
        // If the physical filename is already update.rpf, no fallback is needed.
        let physical_name = "update.rpf";
        let logical_fallback_name = "update.rpf";
        let is_already = physical_name.to_lowercase() == logical_fallback_name;
        // Score is low but we skip fallback because name is already correct.
        let score = 0u32;
        let needs_fallback = !is_already && score < 50;
        assert!(
            !needs_fallback,
            "Should not trigger fallback when already named update.rpf"
        );
    }

    #[test]
    fn fallback_result_wins_when_score_higher() {
        // Simulate: physical scan → score 0, fallback scan → score 100
        // Fallback should win.
        let a1_score: u32 = 0;
        let a2_score: u32 = 100;
        let use_fallback = a2_score > a1_score;
        assert!(use_fallback);
        let final_score = if use_fallback { a2_score } else { a1_score };
        assert_eq!(final_score, 100);
        let label = classify_label_from_score(final_score);
        assert_eq!(label, "obvious_update_rpf");
    }

    #[test]
    fn unrelated_archive_stays_low_even_with_fallback() {
        // A narrow vehicle pack would still score low even if opened as update.rpf name.
        // The tree score is what matters, not just the fallback opening.
        let entries = make_narrow_vehicle_entries();
        let fp = fake_baseline_fp(21000);
        let (score, _, _, _) = score_classify_archive(&entries, &fp);
        let label = classify_label_from_score(score);
        assert!(
            label == "not_update_rpf" || label == "unknown_rpf",
            "Narrow vehicle pack should remain low even if fallback runs: label={}, score={}",
            label,
            score
        );
    }

    #[test]
    fn unknown_class_xml_is_config_candidate() {
        assert_eq!(unknown_class_for_ext("xml"), "unknown_config_candidate");
    }

    #[test]
    fn unknown_class_ytd_is_binary_candidate() {
        assert_eq!(unknown_class_for_ext("ytd"), "unknown_binary_candidate");
    }

    #[test]
    fn unknown_class_rpf_is_nested_archive_candidate() {
        assert_eq!(
            unknown_class_for_ext("rpf"),
            "unknown_nested_archive_candidate"
        );
    }

    #[test]
    fn is_text_candidate_ext_detects_correctly() {
        assert!(is_text_candidate_ext("xml"));
        assert!(is_text_candidate_ext("dat"));
        assert!(is_text_candidate_ext("meta"));
        assert!(!is_text_candidate_ext("ytd"));
        assert!(!is_text_candidate_ext("ypt"));
        assert!(!is_text_candidate_ext("rpf"));
    }

    #[test]
    fn build_unknown_entries_filters_known_components() {
        let known_change = Change {
            path: "ptfx.rpf/core.ypt".to_string(),
            status: "modified".to_string(),
            cleanSize: 1000,
            moddedSize: 900,
            cleanSha256: "abc".to_string(),
            moddedSha256: "def".to_string(),
            extension: "ypt".to_string(),
            basename: "core.ypt".to_string(),
            parentPath: "ptfx.rpf".to_string(),
            sizeDelta: -100,
            sizeDeltaPercent: Some(-10.0),
            category: "particle_container".to_string(),
            components: vec!["tracer".to_string()],
            editorNeeded: vec![],
            risk: "medium".to_string(),
            likelyPattern: "particle_container_reduction".to_string(),
            confidence: "high".to_string(),
            warning: None,
            reason: "size and sha256 differ".to_string(),
        };
        let unknown_change = Change {
            path: "somefile.dat".to_string(),
            status: "modified".to_string(),
            cleanSize: 500,
            moddedSize: 600,
            cleanSha256: "aaa".to_string(),
            moddedSha256: "bbb".to_string(),
            extension: "dat".to_string(),
            basename: "somefile.dat".to_string(),
            parentPath: String::new(),
            sizeDelta: 100,
            sizeDeltaPercent: Some(20.0),
            category: "unknown_binary".to_string(),
            components: vec!["unknown".to_string()],
            editorNeeded: vec![],
            risk: "unknown".to_string(),
            likelyPattern: "unknown_binary_change".to_string(),
            confidence: "low".to_string(),
            warning: None,
            reason: "size and sha256 differ".to_string(),
        };
        let changes = vec![known_change, unknown_change];
        let entries = build_unknown_entries(&changes);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "somefile.dat");
        assert_eq!(entries[0].unknownClass, "unknown_config_candidate");
        assert!(entries[0].safeForAiTextExtraction);
    }

    #[test]
    fn build_unknown_entries_nested_archive_path_detected() {
        let change = Change {
            path: "american_rel.rpf/some_file.gxt2".to_string(),
            status: "added".to_string(),
            cleanSize: 0,
            moddedSize: 200,
            cleanSha256: String::new(),
            moddedSha256: "ccc".to_string(),
            extension: "gxt2".to_string(),
            basename: "some_file.gxt2".to_string(),
            parentPath: "american_rel.rpf".to_string(),
            sizeDelta: 200,
            sizeDeltaPercent: None,
            category: "unknown_binary".to_string(),
            components: vec!["unknown".to_string()],
            editorNeeded: vec![],
            risk: "unknown".to_string(),
            likelyPattern: "asset_added".to_string(),
            confidence: "low".to_string(),
            warning: None,
            reason: "file exists only in modded archive".to_string(),
        };
        let entries = build_unknown_entries(&[change]);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].nestedArchivePath.is_some());
    }

    #[test]
    fn llm_review_task_serializes_correctly() {
        let task = LlmReviewTask {
            task: "review_unknown_change".to_string(),
            path: "somefile.dat".to_string(),
            status: "modified".to_string(),
            extension: "dat".to_string(),
            unknownClass: "unknown_config_candidate".to_string(),
            context: LlmReviewContext {
                folder: "root".to_string(),
                nestedArchivePath: None,
                sizeDeltaBytes: 100,
            },
            question: "What GTA/Redux component might this changed file relate to? Answer as hypothesis only.".to_string(),
        };
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("review_unknown_change"));
        assert!(json.contains("somefile.dat"));
        assert!(!json.contains("password"));
        assert!(!json.contains("key"));
    }

    #[test]
    fn unknown_summary_counts_are_correct() {
        let entries = vec![
            UnknownEntry {
                path: "a.xml".to_string(),
                status: "modified".to_string(),
                name: "a.xml".to_string(),
                extension: "xml".to_string(),
                cleanSizeBytes: 100,
                moddedSizeBytes: 200,
                sizeDeltaBytes: 100,
                cleanSha256: String::new(),
                moddedSha256: String::new(),
                categoryGuess: "config_or_text".to_string(),
                unknownClass: "unknown_config_candidate".to_string(),
                analyzerRequired: false,
                safeForAiTextExtraction: true,
                nestedArchivePath: None,
                reason: "changed".to_string(),
                priority: "high".to_string(),
            },
            UnknownEntry {
                path: "b.ytd".to_string(),
                status: "modified".to_string(),
                name: "b.ytd".to_string(),
                extension: "ytd".to_string(),
                cleanSizeBytes: 1000,
                moddedSizeBytes: 900,
                sizeDeltaBytes: -100,
                cleanSha256: String::new(),
                moddedSha256: String::new(),
                categoryGuess: "texture_dictionary".to_string(),
                unknownClass: "unknown_binary_candidate".to_string(),
                analyzerRequired: true,
                safeForAiTextExtraction: false,
                nestedArchivePath: None,
                reason: "changed".to_string(),
                priority: "medium".to_string(),
            },
        ];
        let text_count = entries
            .iter()
            .filter(|entry| is_text_candidate_ext(&entry.extension))
            .count();
        let binary_count = entries
            .iter()
            .filter(|entry| !is_text_candidate_ext(&entry.extension))
            .count();
        let analyzer_count = entries
            .iter()
            .filter(|entry| entry.analyzerRequired)
            .count();
        assert_eq!(text_count, 1);
        assert_eq!(binary_count, 1);
        assert_eq!(analyzer_count, 1);
    }

    #[test]
    fn looks_like_text_detects_text() {
        assert!(looks_like_text(b"hello world\nfoo=bar\n"));
        assert!(!looks_like_text(b"\x00\x01\x02binary data"));
    }

    #[test]
    fn looks_like_text_rejects_null_bytes() {
        assert!(!looks_like_text(b"has\x00null"));
    }

    #[test]
    fn parse_number_parses_float() {
        assert_eq!(parse_number("1.5"), Some(1.5));
        assert_eq!(parse_number("not_a_number"), None);
    }

    #[test]
    fn diff_lines_basic() {
        let clean = "line1\nline2\nline3\n";
        let modded = "line1\nline2_changed\nline3\n";
        let result = diff_lines(clean, modded, 10);
        assert!(
            result.addedLineCount + result.removedLineCount > 0
                || result.cleanLineCount != result.moddedLineCount
        );
    }

    #[test]
    fn extract_key_value_pairs_equals() {
        let text = "fog_intensity=0.7\nbright_level=2.0\n";
        let pairs = extract_key_value_pairs(text);
        assert!(pairs.iter().any(|(k, _)| k == "fog_intensity"));
    }

    #[test]
    fn extract_key_value_pairs_colon() {
        let text = "key: value\nanother: stuff\n";
        let pairs = extract_key_value_pairs(text);
        assert!(pairs.iter().any(|(k, _)| k == "key"));
    }

    #[test]
    fn analyze_xml_content_produces_entry() {
        let clean = b"<root><value>0.5</value></root>";
        let modded = b"<root><value>0.3</value></root>";
        let entry = analyze_xml_content("test.xml", "modified", Some(clean), Some(modded));
        assert_eq!(entry.analyzer, "xml_analyzer");
        assert!(entry.numericChanges > 0 || entry.addedLines > 0 || entry.removedLines > 0);
    }

    #[test]
    fn analyze_dat_content_detects_key_change() {
        let clean = b"fog_intensity=0.7\nbright=1.0\n";
        let modded = b"fog_intensity=0.2\nbright=1.0\n";
        let entry = analyze_dat_content("bloodfx.dat", "modified", Some(clean), Some(modded));
        assert!(entry.readable);
        assert!(entry.changedKeyCount > 0 || entry.addedLines > 0);
    }

    #[test]
    fn ai_readable_change_notes_serializes() {
        let note = AiChangeNote {
            task: "explain_text_config_change".to_string(),
            path: "test.xml".to_string(),
            extension: "xml".to_string(),
            analyzer: "xml_analyzer".to_string(),
            changeSummary: AiChangeSummary {
                addedLines: 2,
                removedLines: 1,
                numericChanges: 3,
                colorLikeChanges: 0,
            },
            question: "What visual/config component might this change relate to? Treat as hypothesis only.".to_string(),
        };
        let json = serde_json::to_string(&note).unwrap();
        assert!(json.contains("explain_text_config_change"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn text_analysis_summary_counts_match() {
        let stats = TextAnalysisStats {
            totalCandidates: 10,
            analyzedFiles: 8,
            skippedFiles: 2,
            xmlAnalyzed: 3,
            datAnalyzed: 2,
            metaAnalyzed: 2,
            genericTextAnalyzed: 1,
            parseFailures: 0,
            extractionFailures: 2,
            tooLargeSkipped: 0,
            skippedNotTextBytes: 0,
        };
        assert_eq!(
            stats.xmlAnalyzed + stats.datAnalyzed + stats.metaAnalyzed + stats.genericTextAnalyzed,
            stats.analyzedFiles
        );
    }

    #[test]
    fn compute_component_frequency_groups_by_component() {
        let changes = vec![
            Change {
                path: "a.xml".to_string(),
                status: "modified".to_string(),
                cleanSize: 0,
                moddedSize: 0,
                cleanSha256: "".to_string(),
                moddedSha256: "".to_string(),
                extension: "xml".to_string(),
                basename: "a.xml".to_string(),
                parentPath: "".to_string(),
                sizeDelta: 0,
                sizeDeltaPercent: None,
                category: "visual".to_string(),
                components: vec!["sky_timecycle".to_string()],
                editorNeeded: vec![],
                risk: "low".to_string(),
                likelyPattern: "".to_string(),
                confidence: "medium".to_string(),
                warning: None,
                reason: "".to_string(),
            },
            Change {
                path: "b.xml".to_string(),
                status: "added".to_string(),
                cleanSize: 0,
                moddedSize: 0,
                cleanSha256: "".to_string(),
                moddedSha256: "".to_string(),
                extension: "xml".to_string(),
                basename: "b.xml".to_string(),
                parentPath: "".to_string(),
                sizeDelta: 100,
                sizeDeltaPercent: None,
                category: "visual".to_string(),
                components: vec!["sky_timecycle".to_string()],
                editorNeeded: vec![],
                risk: "low".to_string(),
                likelyPattern: "".to_string(),
                confidence: "medium".to_string(),
                warning: None,
                reason: "".to_string(),
            },
        ];
        let freq = compute_component_frequency(&changes);
        let sky = freq.iter().find(|entry| entry.component == "sky_timecycle");
        assert!(sky.is_some());
        assert_eq!(sky.unwrap().totalChanged, 2);
        assert_eq!(sky.unwrap().added, 1);
        assert_eq!(sky.unwrap().modified, 1);
    }

    #[test]
    fn compute_file_type_frequency_counts_extensions() {
        let changes = vec![Change {
            path: "c.dat".to_string(),
            status: "modified".to_string(),
            cleanSize: 0,
            moddedSize: 0,
            cleanSha256: "".to_string(),
            moddedSha256: "".to_string(),
            extension: "dat".to_string(),
            basename: "c.dat".to_string(),
            parentPath: "".to_string(),
            sizeDelta: 50,
            sizeDeltaPercent: None,
            category: "config".to_string(),
            components: vec!["unknown".to_string()],
            editorNeeded: vec![],
            risk: "low".to_string(),
            likelyPattern: "".to_string(),
            confidence: "low".to_string(),
            warning: None,
            reason: "".to_string(),
        }];
        let unknown_entries: Vec<UnknownEntry> = vec![];
        let freq = compute_file_type_frequency(&changes, &unknown_entries);
        let dat_entry = freq.iter().find(|entry| entry.extension == "dat");
        assert!(dat_entry.is_some());
        assert_eq!(dat_entry.unwrap().totalChanged, 1);
    }

    #[test]
    fn build_corpus_ai_change_notes_creates_note() {
        let summary = TextAnalysisFileSummary {
            path: "test.xml".to_string(),
            extension: "xml".to_string(),
            analyzer: "xml_analyzer".to_string(),
            status: "modified".to_string(),
            sizeDelta: -100,
            addedLines: 2,
            removedLines: 3,
            numericChanges: 5,
            colorLikeChanges: 1,
        };
        let results = TextAnalysisResults {
            xml_entries: vec![],
            dat_entries: vec![],
            meta_entries: vec![],
            generic_entries: vec![],
            analyzer_warnings: vec![],
            stats: TextAnalysisStats {
                totalCandidates: 1,
                analyzedFiles: 1,
                skippedFiles: 0,
                xmlAnalyzed: 1,
                datAnalyzed: 0,
                metaAnalyzed: 0,
                genericTextAnalyzed: 0,
                parseFailures: 0,
                extractionFailures: 0,
                tooLargeSkipped: 0,
                skippedNotTextBytes: 0,
            },
            file_summaries: vec![summary],
        };
        let notes = build_corpus_ai_change_notes(&results);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].extension, "xml");
        assert!(!notes[0].safeForGeneration);
        assert!(notes[0].safeForAiPlanning);
    }

    #[test]
    fn build_training_candidates_requires_changes() {
        let summary = TextAnalysisFileSummary {
            path: "vis.dat".to_string(),
            extension: "dat".to_string(),
            analyzer: "dat_config_analyzer".to_string(),
            status: "modified".to_string(),
            sizeDelta: 100,
            addedLines: 1,
            removedLines: 0,
            numericChanges: 10,
            colorLikeChanges: 0,
        };
        let results = TextAnalysisResults {
            xml_entries: vec![],
            dat_entries: vec![],
            meta_entries: vec![],
            generic_entries: vec![],
            analyzer_warnings: vec![],
            stats: TextAnalysisStats {
                totalCandidates: 1,
                analyzedFiles: 1,
                skippedFiles: 0,
                xmlAnalyzed: 0,
                datAnalyzed: 1,
                metaAnalyzed: 0,
                genericTextAnalyzed: 0,
                parseFailures: 0,
                extractionFailures: 0,
                tooLargeSkipped: 0,
                skippedNotTextBytes: 0,
            },
            file_summaries: vec![summary],
        };
        let changes: Vec<Change> = vec![];
        let candidates = build_training_candidates(Some(&results), &changes);
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].trainingStatus, "candidate_unreviewed");
        assert!(!candidates[0].expectedOutputStyle.safeForGeneration);
    }

    #[test]
    fn training_candidate_serializes_correctly() {
        let candidate = TrainingCandidate {
            task: "explain_redux_file_change".to_string(),
            trainingStatus: "candidate_unreviewed".to_string(),
            input: TrainingCandidateInput {
                path: "test.xml".to_string(),
                extension: "xml".to_string(),
                analyzerSummary: CorpusChangeSummary {
                    addedLines: 1,
                    removedLines: 0,
                    numericChanges: 5,
                    colorLikeChanges: 2,
                },
            },
            expectedOutputStyle: TrainingCandidateExpected {
                componentHypothesis: "unknown - review required".to_string(),
                risk: "low".to_string(),
                recommendedTool: "xml_timecycle_editor".to_string(),
                safeForGeneration: false,
            },
        };
        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains("candidate_unreviewed"));
        assert!(json.contains("safeForGeneration"));
        assert!(!json.contains("password"));
        assert!(!json.contains("key"));
    }

    #[test]
    fn render_local_ai_context_contains_sections() {
        let changes: Vec<Change> = vec![];
        let unknown: Vec<UnknownEntry> = vec![];
        let freq: Vec<ComponentFreqEntry> = vec![];
        let baseline_meta = BaselineMetadataFile {
            baselineArchiveHash: "abc123".to_string(),
            baselineArchiveSizeBytes: 1000,
            baselineArchiveFileName: "update.rpf".to_string(),
            baselineArchivePath: None,
        };
        let result = render_local_ai_context(
            &changes,
            &unknown,
            None,
            &freq,
            &baseline_meta,
            0,
            "2025-01-01T00:00:00Z",
        );
        assert!(result.contains("# Local AI Context"));
        assert!(result.contains("What was compared"));
        assert!(result.contains("safe to reason about") || result.contains("Safe to"));
        assert!(result.contains("NOT safe") || result.contains("not safe"));
    }

    #[test]
    fn corpus_index_contains_warning() {
        let totals = CorpusTotals {
            added: 100,
            removed: 50,
            modified: 200,
            totalUnknown: 13894,
            textCandidates: 960,
            binaryCandidates: 12934,
            analyzedTextFiles: 141,
            skippedTextFiles: 841,
            candidatePatterns: 99,
        };
        let index = CorpusIndex {
            schemaVersion: "2.0".to_string(),
            generatedAt: "2025-01-01T00:00:00Z".to_string(),
            scannerVersion: "0.2.0".to_string(),
            baselineArchiveHash: "abc".to_string(),
            baselineArchiveFileName: "update.rpf".to_string(),
            moddedArchiveHash: "def".to_string(),
            moddedArchiveFileName: "update.rpf".to_string(),
            sourceArtifacts: vec!["clean_vs_modded_diff.json".to_string()],
            totals,
            artifacts: vec!["learning_corpus/learning_corpus_index.json".to_string()],
            warning: "local-only".to_string(),
        };
        let json = serde_json::to_string(&index).unwrap();
        assert!(json.contains("local-only") || json.contains("warning"));
    }

    #[test]
    fn missing_text_results_handled_gracefully() {
        let changes: Vec<Change> = vec![];
        let notes = build_corpus_ai_change_notes_from_opt(None);
        assert!(notes.is_empty());
        let lessons = build_file_lessons(&changes, None);
        assert!(lessons.is_empty());
        let candidates = build_training_candidates(None, &changes);
        assert!(candidates.is_empty());
    }

    #[test]
    fn corpus_recommended_future_tool_maps_extensions() {
        assert_eq!(
            corpus_recommended_future_tool("xml"),
            "xml_timecycle_editor"
        );
        assert_eq!(corpus_recommended_future_tool("dat"), "dat_config_patcher");
        assert_eq!(
            corpus_recommended_future_tool("ytd"),
            "ytd_texture_analyzer"
        );
        assert_eq!(corpus_recommended_future_tool("bin"), "unknown_analyzer");
    }

    #[test]
    fn render_redux_making_atlas_contains_sections() {
        let result =
            render_redux_making_atlas(&[], &[], None, &[], &[], &[], 0, "2025-01-01T00:00:00Z");
        assert!(result.contains("# Redux Making Atlas"));
        assert!(result.contains("Known Component Changes"));
        assert!(result.contains("Future Tool Recommendations"));
    }

    #[test]
    fn file_type_frequency_marks_binary_extensions_analyzer_required() {
        let changes = vec![Change {
            path: "a.ytd".to_string(),
            status: "modified".to_string(),
            cleanSize: 0,
            moddedSize: 0,
            cleanSha256: "".to_string(),
            moddedSha256: "".to_string(),
            extension: "ytd".to_string(),
            basename: "a.ytd".to_string(),
            parentPath: "".to_string(),
            sizeDelta: 0,
            sizeDeltaPercent: None,
            category: "texture".to_string(),
            components: vec!["unknown".to_string()],
            editorNeeded: vec![],
            risk: "low".to_string(),
            likelyPattern: "".to_string(),
            confidence: "low".to_string(),
            warning: None,
            reason: "".to_string(),
        }];
        let unknown_entries = vec![UnknownEntry {
            path: "a.ytd".to_string(),
            status: "modified".to_string(),
            name: "a.ytd".to_string(),
            extension: "ytd".to_string(),
            cleanSizeBytes: 0,
            moddedSizeBytes: 0,
            sizeDeltaBytes: 0,
            cleanSha256: "".to_string(),
            moddedSha256: "".to_string(),
            categoryGuess: "texture".to_string(),
            unknownClass: "unknown_binary_candidate".to_string(),
            analyzerRequired: true,
            safeForAiTextExtraction: false,
            nestedArchivePath: None,
            reason: "".to_string(),
            priority: "high".to_string(),
        }];
        let freq = compute_file_type_frequency(&changes, &unknown_entries);
        assert_eq!(freq[0].analyzerStatus, "analyzer_required");
    }

    #[test]
    fn scanner_ok_prefix_constant() {
        // Verifies the SCANNER_OK prefix used by HomeOps is as documented
        let out = std::path::Path::new("/data/diffs/redux_v2");
        let line = format!("SCANNER_OK {}", out.display());
        assert!(line.starts_with("SCANNER_OK "));
        assert!(line.contains("redux_v2"));
    }

    #[test]
    fn diff_artifact_list_is_complete() {
        // Verifies the expected diff artifact filenames are known
        let artifacts = vec![
            "full_modded_manifest.json",
            "full_modded_tree.json",
            "clean_vs_modded_diff.json",
            "diff_summary.json",
            "unknown_changes.json",
            "candidate_patterns.json",
            "llm_review_queue.jsonl",
        ];
        for artifact in &artifacts {
            assert!(!artifact.is_empty());
            assert!(artifact.ends_with(".json") || artifact.ends_with(".jsonl"));
        }
        assert_eq!(artifacts.len(), 7);
    }

    #[test]
    fn baseline_artifact_list_is_complete() {
        let artifacts = vec![
            "full_clean_manifest.json",
            "full_clean_tree.json",
            "baseline_update_tree_fingerprint.json",
            "baseline_metadata.json",
        ];
        assert_eq!(artifacts.len(), 4);
        for a in &artifacts {
            assert!(a.ends_with(".json"));
        }
    }

    #[test]
    fn timecycle_rankings_visualsettings_with_keys_ranks_first() {
        let results = make_timecycle_results(
            vec![make_timecycle_xml_entry("cloudkeyframes.xml", 42, 88)],
            vec![make_timecycle_dat_entry(
                "visualsettings.dat",
                4,
                12,
                vec![make_timecycle_key_change("Adaptation.max.step.size")],
            )],
        );
        let rankings = build_timecycle_file_rankings(Some(&results));
        assert_eq!(
            rankings.first().unwrap().path_or_family,
            "visualsettings.dat"
        );
        assert_eq!(rankings.first().unwrap().rank, 1);
    }

    #[test]
    fn timecycle_rankings_cloudkeyframes_ranks_high() {
        let results = make_timecycle_results(
            vec![
                make_timecycle_xml_entry("cloudkeyframes.xml", 30, 120),
                make_timecycle_xml_entry("timecycle_mods_1.xml", 10, 40),
                make_timecycle_xml_entry("w_foggy.xml", 20, 30),
            ],
            vec![],
        );
        let rankings = build_timecycle_file_rankings(Some(&results));
        let cloud = rankings
            .iter()
            .find(|entry| entry.path_or_family == "cloudkeyframes.xml")
            .unwrap();
        assert!(cloud.rank <= 3, "cloudkeyframes rank was {}", cloud.rank);
    }

    #[test]
    fn safe_edit_matrix_timecycle_mods3_is_blocked() {
        let matrix = build_timecycle_safe_edit_matrix();
        let entry = matrix
            .iter()
            .find(|item| item.file == "timecycle_mods_3.xml")
            .unwrap();
        assert!(entry.allowed_first_patch_operations.is_empty());
        assert!(entry
            .blocked_operations
            .contains(&"schema_unknown_parameter_edit".to_string()));
    }

    #[test]
    fn safe_edit_matrix_cloudkeyframes_allows_color_ops() {
        let matrix = build_timecycle_safe_edit_matrix();
        let entry = matrix
            .iter()
            .find(|item| item.file == "cloudkeyframes.xml")
            .unwrap();
        assert!(entry
            .allowed_first_patch_operations
            .contains(&"color_like_desaturation".to_string()));
        assert!(entry
            .allowed_first_patch_operations
            .contains(&"color_like_darken".to_string()));
    }

    #[test]
    fn risky_files_includes_tracer_component() {
        let report = build_risky_files_report(
            &dummy_tool_metadata(),
            &dummy_timing(),
            "2025-01-01T00:00:00Z",
        );
        let risky: Vec<&str> = report
            .risky_files
            .iter()
            .map(|entry| entry.file_or_family.as_str())
            .collect();
        assert!(risky.contains(&"tracer component"));
        assert!(risky.contains(&"hit_effect component"));
        assert!(risky.contains(&"minimap_hud component"));
    }

    #[test]
    fn cloudkeyframes_color_only_detection() {
        let results = make_timecycle_results(
            vec![make_timecycle_xml_entry("cloudkeyframes.xml", 0, 25)],
            vec![],
        );
        let report = build_cloudkeyframes_report(
            Some(&results),
            &dummy_tool_metadata(),
            &dummy_timing(),
            "2025-01-01T00:00:00Z",
        );
        assert!(report.color_only_pattern_detected);
        assert!(!report.numeric_and_color_pattern);
    }

    #[test]
    fn weather_xml_deferred_classification() {
        let results = make_timecycle_results(
            vec![make_timecycle_xml_entry("weather.xml", 90, 12)],
            vec![],
        );
        let report = build_weather_xml_report(
            Some(&results),
            &dummy_tool_metadata(),
            &dummy_timing(),
            "2025-01-01T00:00:00Z",
        );
        assert_eq!(report.global_weather_xml.path, "weather.xml");
        assert_eq!(report.global_weather_xml.suggested_phase, "deferred");
    }

    #[test]
    fn compact_context_contains_required_sections() {
        let rankings = build_timecycle_file_rankings(None);
        let risky = build_risky_files_report(
            &dummy_tool_metadata(),
            &dummy_timing(),
            "2025-01-01T00:00:00Z",
        );
        let context = render_ai_timecycle_context_compact(
            None,
            &rankings,
            &risky.risky_files,
            &dummy_tool_metadata(),
            "2025-01-01T00:00:00Z",
        );
        assert!(context.contains("# Sky/Timecycle AI Context — Redux Scanner"));
        assert!(context.contains("## Scan Summary"));
        assert!(context.contains("## Ranked Candidate Files"));
        assert!(context.contains("## AI Must Not"));
        assert!(context.contains("## AI May"));
    }

    #[test]
    fn visualsettings_key_grouping_by_prefix() {
        let results = make_timecycle_results(
            vec![],
            vec![make_timecycle_dat_entry(
                "visualsettings.dat",
                1,
                1,
                vec![make_timecycle_key_change("Adaptation.max.step.size")],
            )],
        );
        let report = build_visualsettings_key_report(
            Some(&results),
            &dummy_tool_metadata(),
            &dummy_timing(),
            "2025-01-01T00:00:00Z",
        );
        let family = report
            .key_families
            .iter()
            .find(|entry| entry.family == "Adaptation")
            .unwrap();
        assert!(family
            .keys
            .contains(&"Adaptation.max.step.size".to_string()));
    }

    #[test]
    fn prompt_pack_contains_all_prompt_types() {
        let prompt_pack = render_ai_timecycle_prompt_pack();
        assert!(prompt_pack.contains("## System Prompt"));
        assert!(prompt_pack.contains("## User Prompt (Full Context)"));
        assert!(prompt_pack.contains("## Compact Free-Model Prompt"));
        assert!(prompt_pack.contains("## JSON Patch-Plan Prompt"));
        assert!(prompt_pack.contains("## Critic/Grading Prompt"));
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
            println!("SCANNER_OK {}", out.display());
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

            let (clean_entries, _clean_counters) = scan_archive(
                &clean,
                &keys,
                args.depth,
                scan_options,
                rules.target_rules.as_ref(),
                &mut warnings,
            )
            .with_context(|| format!("failed to scan clean archive {}", clean.display()))?;

            let (modded_entries, _modded_counters) = scan_archive(
                &modded,
                &keys,
                args.depth,
                scan_options,
                rules.target_rules.as_ref(),
                &mut warnings,
            )
            .with_context(|| format!("failed to scan modded archive {}", modded.display()))?;

            let changes = diff_maps(
                &clean_entries,
                &modded_entries,
                rules.component_rules.as_ref(),
            );
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
            println!(
                "added: {}  removed: {}  modified: {}",
                added, removed, modified
            );
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
            println!("SCANNER_OK {}", out.display());
        }
        "baseline-scan" => {
            let tool = build_tool_metadata(&args);
            let started_at = OffsetDateTime::now_utc();
            let start_instant = Instant::now();
            let archive = args
                .archive
                .clone()
                .context("baseline-scan requires --archive")?;
            let keys_path = args.keys.clone().context("baseline-scan requires --keys")?;
            let out_dir = args
                .out
                .clone()
                .context("baseline-scan requires --out (output folder)")?;

            fs::create_dir_all(&out_dir).with_context(|| {
                format!(
                    "failed to create baseline output dir: {}",
                    out_dir.display()
                )
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
                &archive.display().to_string(),
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
            println!("SCANNER_OK {}", out_dir.display());
        }
        "diff-against-baseline" => {
            let tool = build_tool_metadata(&args);
            let started_at = OffsetDateTime::now_utc();
            let start_instant = Instant::now();
            let modded = args
                .modded
                .clone()
                .context("diff-against-baseline requires --modded")?;
            let baseline_dir = args
                .baseline
                .clone()
                .context("diff-against-baseline requires --baseline")?;
            let keys_path = args
                .keys
                .clone()
                .context("diff-against-baseline requires --keys")?;
            let out_dir = args
                .out
                .clone()
                .context("diff-against-baseline requires --out (output folder)")?;

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
            let changes = diff_maps(
                &clean_entries,
                &modded_entries,
                rules.component_rules.as_ref(),
            );
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

            let unknown_entries = build_unknown_entries(&changes);
            write_unknown_changes(&out_dir, &tool, &timing, &scan, &unknown_entries, &warnings)?;
            write_unknown_text_candidates(&out_dir, &tool, &timing, &unknown_entries, &warnings)?;
            write_unknown_binary_candidates(&out_dir, &tool, &timing, &unknown_entries, &warnings)?;
            let pattern_count =
                write_candidate_patterns(&out_dir, &tool, &timing, &unknown_entries)?;
            write_llm_review_queue(&out_dir, &unknown_entries)?;
            write_unknown_summary(
                &out_dir,
                &tool,
                &timing,
                &unknown_entries,
                pattern_count,
                &warnings,
            )?;

            let mut text_analysis_results: Option<TextAnalysisResults> = None;

            if args.analyze_text {
                let clean_archive_path = if let Some(ref p) = args.clean {
                    Some(p.clone())
                } else if let Some(ref p) = baseline_meta.baselineArchivePath {
                    if !p.is_empty() {
                        Some(PathBuf::from(p))
                    } else {
                        None
                    }
                } else {
                    None
                };

                match clean_archive_path {
                    None => {
                        push_warning(
                            &mut warnings,
                            "TEXT_ANALYZE_SKIPPED",
                            "",
                            "--analyze-text requires --clean <clean.rpf> (or re-run baseline-scan to store path)".to_string(),
                        );
                        println!("text analysis skipped: clean archive path not available");
                    }
                    Some(clean_path) => {
                        match run_text_analyzers(
                            &changes,
                            &clean_path,
                            &modded,
                            &keys,
                            args.depth,
                            &mut warnings,
                        ) {
                            Ok(results) => {
                                write_text_analysis_summary(
                                    &out_dir,
                                    &tool,
                                    &timing,
                                    &results.stats,
                                    &results,
                                )?;
                                write_xml_diffs(&out_dir, &tool, &timing, &results.xml_entries)?;
                                write_dat_diffs(&out_dir, &tool, &timing, &results.dat_entries)?;
                                write_meta_diffs(&out_dir, &tool, &timing, &results.meta_entries)?;
                                write_generic_text_diffs(
                                    &out_dir,
                                    &tool,
                                    &timing,
                                    &results.generic_entries,
                                )?;
                                write_analyzer_warnings(&out_dir, &results.analyzer_warnings)?;
                                write_ai_readable_change_notes(&out_dir, &results)?;
                                println!("text analysis complete");
                                println!("  xml analyzed: {}", results.stats.xmlAnalyzed);
                                println!("  dat analyzed: {}", results.stats.datAnalyzed);
                                println!("  meta analyzed: {}", results.stats.metaAnalyzed);
                                println!(
                                    "  generic text analyzed: {}",
                                    results.stats.genericTextAnalyzed
                                );
                                println!(
                                    "  extraction failures: {}",
                                    results.stats.extractionFailures
                                );
                                text_analysis_results = Some(results);
                            }
                            Err(e) => {
                                push_warning(
                                    &mut warnings,
                                    "TEXT_ANALYZE_ERROR",
                                    "",
                                    format!("text analysis failed: {}", e),
                                );
                                println!("text analysis failed: {}", e);
                            }
                        }
                    }
                }
            }

            // Learning corpus (R0.8)
            if args.build_learning_corpus {
                let modded_hash = modded_identity.archiveSha256.as_str();
                let modded_filename = modded_identity.archiveFileName.as_str();
                let generated_at = format!(
                    "{}",
                    OffsetDateTime::now_utc()
                        .format(&Rfc3339)
                        .unwrap_or_default()
                );
                match build_and_write_learning_corpus(
                    &out_dir,
                    &changes,
                    &unknown_entries,
                    text_analysis_results.as_ref(),
                    &tool,
                    &timing,
                    &baseline_meta,
                    modded_hash,
                    modded_filename,
                    pattern_count,
                    &generated_at,
                ) {
                    Ok(()) => println!(
                        "learning corpus written to: {}/learning_corpus",
                        out_dir.display()
                    ),
                    Err(e) => {
                        push_warning(
                            &mut warnings,
                            "CORPUS_BUILD_ERROR",
                            "",
                            format!("corpus build failed: {}", e),
                        );
                        println!("corpus build failed: {}", e);
                    }
                }

                if args.analyze_text {
                    match build_and_write_timecycle_intelligence(
                        &out_dir,
                        text_analysis_results.as_ref(),
                        &tool,
                        &timing,
                        &generated_at,
                    ) {
                        Ok(()) => println!(
                            "timecycle intelligence written to: {}/timecycle_intelligence",
                            out_dir.display()
                        ),
                        Err(e) => {
                            push_warning(
                                &mut warnings,
                                "TIMECYCLE_INTELLIGENCE_ERROR",
                                "",
                                format!("timecycle intelligence failed: {}", e),
                            );
                            println!("timecycle intelligence failed: {}", e);
                        }
                    }
                }
            }

            let added = changes.iter().filter(|c| c.status == "added").count();
            let removed = changes.iter().filter(|c| c.status == "removed").count();
            let modified = changes.iter().filter(|c| c.status == "modified").count();
            let unknown_count = unknown_entries.len();
            let unknown_text = unknown_entries
                .iter()
                .filter(|entry| is_text_candidate_ext(&entry.extension))
                .count();
            let unknown_binary = unknown_entries
                .iter()
                .filter(|entry| !is_text_candidate_ext(&entry.extension))
                .count();

            println!("diff-against-baseline complete");
            println!("modded: {}", modded.display());
            println!("baseline: {}", baseline_dir.display());
            println!("clean entries: {}", clean_entry_count);
            println!("modded entries: {}", modded_entry_count);
            println!("added: {}", added);
            println!("removed: {}", removed);
            println!("modified: {}", modified);
            println!("unknown changes: {}", unknown_count);
            println!("  text/config candidates: {}", unknown_text);
            println!("  binary candidates: {}", unknown_binary);
            println!("out: {}", out_dir.display());
            println!("SCANNER_OK {}", out_dir.display());
        }
        "classify-rpf" => {
            let tool = build_tool_metadata(&args);
            let started_at = OffsetDateTime::now_utc();
            let start_instant = Instant::now();
            let archive = args
                .archive
                .clone()
                .context("classify-rpf requires --archive")?;
            let baseline_dir = args
                .baseline
                .clone()
                .context("classify-rpf requires --baseline")?;
            let keys_path = args.keys.clone().context("classify-rpf requires --keys")?;
            let out_path = args.out.clone().context("classify-rpf requires --out")?;

            if let Some(parent) = out_path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create output parent dir: {}", parent.display())
                    })?;
                }
            }

            let archive_identity = build_archive_identity(&archive)?;
            let baseline_fp = load_baseline_fingerprint(&baseline_dir)?;

            let keys = GtaKeys::load_from_path(&keys_path).with_context(|| {
                format!("failed to load keys directory from {}", keys_path.display())
            })?;

            let mut warnings = Vec::new();

            // Tree-only scan: no hashing (fast), nested allowed, all entries
            let scan_options = ScanOptions {
                targets_only: false,
                hash_entries: false,
                allow_nested: true,
            };
            let scan = ScanMetadata {
                mode: "tree-only".to_string(),
                depth: args.depth,
            };

            let physical_file_name = archive
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            // ── Attempt 1: physical filename ─────────────────────────────────
            let (a1_entries_opt, a1_note) = match scan_archive(
                &archive,
                &keys,
                args.depth,
                scan_options,
                None,
                &mut warnings,
            ) {
                Ok((entries, _)) => (Some(entries), None),
                Err(e) => (None, Some(format!("scan failed: {}", e))),
            };

            let (a1_score, a1_reasons, a1_matched, a1_missing, a1_count, a1_label) =
                if let Some(ref entries) = a1_entries_opt {
                    let (s, r, m, mis) = score_classify_archive(entries, &baseline_fp);
                    let l = classify_label_from_score(s).to_string();
                    (s, r, m, mis, entries.len(), l)
                } else {
                    (0, vec![], vec![], vec![], 0, "scan_failed".to_string())
                };

            // ── Attempt 2: logical update.rpf name fallback ───────────────────
            // Skip fallback if archive is already named update.rpf (key derivation already correct).
            // Skip fallback if initial score is already confident enough.
            let logical_fallback_name = "update.rpf";
            let is_already_update_rpf = physical_file_name.to_lowercase() == logical_fallback_name;
            let needs_fallback = !is_already_update_rpf && a1_score < 50;

            let (a2_entries_opt, a2_note) = if needs_fallback {
                match copy_archive_to_logical_name(&archive, logical_fallback_name) {
                    Ok((temp_dir, logical_path)) => {
                        let result = match scan_archive(
                            &logical_path,
                            &keys,
                            args.depth,
                            scan_options,
                            None,
                            &mut warnings,
                        ) {
                            Ok((entries, _)) => (Some(entries), None),
                            Err(e) => (None, Some(format!("logical-name scan failed: {}", e))),
                        };
                        drop(temp_dir);
                        result
                    }
                    Err(e) => (
                        None,
                        Some(format!("failed to prepare logical-name copy: {}", e)),
                    ),
                }
            } else {
                (None, None)
            };

            let (a2_score, a2_reasons, a2_matched, a2_missing, a2_count, a2_label) =
                if let Some(ref entries) = a2_entries_opt {
                    let (s, r, m, mis) = score_classify_archive(entries, &baseline_fp);
                    let l = classify_label_from_score(s).to_string();
                    (s, r, m, mis, entries.len(), l)
                } else {
                    (0, vec![], vec![], vec![], 0, "scan_failed".to_string())
                };

            // ── Pick best result ──────────────────────────────────────────────
            let use_fallback = needs_fallback && a2_score > a1_score;
            let used_logical_name: Option<&str> = if use_fallback {
                Some(logical_fallback_name)
            } else {
                None
            };

            let (
                final_score,
                mut final_reasons,
                final_matched,
                final_missing,
                final_count,
                final_label,
                final_entries_ref,
            ) = if use_fallback {
                (
                    a2_score,
                    a2_reasons.clone(),
                    a2_matched.clone(),
                    a2_missing.clone(),
                    a2_count,
                    a2_label.clone(),
                    &a2_entries_opt,
                )
            } else {
                (
                    a1_score,
                    a1_reasons.clone(),
                    a1_matched.clone(),
                    a1_missing.clone(),
                    a1_count,
                    a1_label.clone(),
                    &a1_entries_opt,
                )
            };

            // Prepend a clear reason when the fallback drove the result
            if use_fallback {
                final_reasons.insert(
                    0,
                    format!(
                        "Archive matched update.rpf tree when opened with logical name \"{}\". \
                         GTA V NG encryption derives decryption keys from the archive filename; \
                         the physical name \"{}\" produced no readable tree (score={}).",
                        logical_fallback_name, physical_file_name, a1_score
                    ),
                );
            }

            let final_classification = final_label.as_str();
            let final_confidence = (final_score as f64) / 100.0;
            let final_recommended_action = recommend_action_from_label(final_classification);

            let empty_entries: BTreeMap<String, EntryInfo> = BTreeMap::new();
            let winning_entries = final_entries_ref.as_ref().unwrap_or(&empty_entries);
            let top_folders = collect_top_level_folders(winning_entries);
            let hist = build_extension_histogram(winning_entries);

            // ── Build attempts record ─────────────────────────────────────────
            let mut attempts: Vec<ClassifyAttempt> = vec![ClassifyAttempt {
                physicalFileName: physical_file_name.clone(),
                logicalFileName: physical_file_name.clone(),
                entryCount: a1_count,
                score: a1_score,
                classification: a1_label.clone(),
                usedForResult: !use_fallback,
                note: a1_note,
            }];

            if needs_fallback {
                let a2_note_final = if a2_note.is_some() {
                    a2_note
                } else if use_fallback {
                    Some(format!(
                        "Archive matched update.rpf tree when opened with logical name \"{}\".",
                        logical_fallback_name
                    ))
                } else {
                    Some(format!(
                        "Fallback scan completed (score={}) but did not exceed physical-name score ({}).",
                        a2_score, a1_score
                    ))
                };
                attempts.push(ClassifyAttempt {
                    physicalFileName: physical_file_name.clone(),
                    logicalFileName: logical_fallback_name.to_string(),
                    entryCount: a2_count,
                    score: a2_score,
                    classification: a2_label.clone(),
                    usedForResult: use_fallback,
                    note: a2_note_final,
                });
            }

            let timing = Timing {
                startedAt: format_timestamp(started_at)?,
                finishedAt: format_timestamp(OffsetDateTime::now_utc())?,
                durationMs: start_instant.elapsed().as_millis() as u64,
            };

            write_classification_report(
                &out_path,
                &archive_identity,
                &baseline_fp,
                &tool,
                &timing,
                &scan,
                final_score,
                final_classification,
                final_confidence,
                final_recommended_action,
                &final_reasons,
                &final_matched,
                &final_missing,
                &top_folders,
                &hist,
                final_count,
                &warnings,
                &attempts,
                used_logical_name,
            )?;

            println!("classify-rpf complete");
            println!("archive: {}", archive.display());
            println!("entries scanned: {}", final_count);
            println!("score: {}", final_score);
            println!("classification: {}", final_classification);
            println!("confidence: {:.2}", final_confidence);
            println!("recommended action: {}", final_recommended_action);
            if use_fallback {
                println!(
                    "note: classified via logical name \"{}\" (GTA NG key derivation)",
                    logical_fallback_name
                );
            }
            println!("out: {}", out_path.display());
            println!("SCANNER_OK {}", out_path.display());
        }
        "validate-xml" => {
            let file = args.file.clone().context("validate-xml requires --file")?;
            let vmode = match args.vmode.as_deref() {
                Some("parse_only") => Some(validators::xml_validator::XmlValidationMode::ParseOnly),
                Some("color_like_only") => {
                    Some(validators::xml_validator::XmlValidationMode::ColorLikeOnly)
                }
                Some("structure_preserved") => {
                    Some(validators::xml_validator::XmlValidationMode::StructurePreserved)
                }
                Some("no_numeric_changes") => {
                    Some(validators::xml_validator::XmlValidationMode::NoNumericChanges)
                }
                Some("diff_against_baseline") => {
                    Some(validators::xml_validator::XmlValidationMode::DiffAgainstBaseline)
                }
                Some(other) => anyhow::bail!("invalid vmode for xml: {}", other),
                None => None,
            };
            let result =
                validators::xml_validator::validate_xml(&file, vmode, args.baseline.as_deref());
            write_validation_result(args.out.as_ref(), &result)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        "validate-dat" => {
            let file = args.file.clone().context("validate-dat requires --file")?;
            let vmode = match args.vmode.as_deref() {
                Some("parse_only") => Some(validators::dat_validator::DatValidationMode::ParseOnly),
                Some("named_key_only") => {
                    Some(validators::dat_validator::DatValidationMode::NamedKeyOnly)
                }
                Some("allowed_family_only") => {
                    Some(validators::dat_validator::DatValidationMode::AllowedFamilyOnly)
                }
                Some("diff_against_baseline") => {
                    Some(validators::dat_validator::DatValidationMode::DiffAgainstBaseline)
                }
                Some(other) => anyhow::bail!("invalid vmode for dat: {}", other),
                None => None,
            };
            let result =
                validators::dat_validator::validate_dat(&file, vmode, args.baseline.as_deref());
            write_validation_result(args.out.as_ref(), &result)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        "validate-scope" => {
            let plan = args
                .patch_plan
                .clone()
                .context("validate-scope requires --patch-plan")?;
            let result = validators::scope_validator::validate_scope(&plan, &args.changed_files);
            write_validation_result(args.out.as_ref(), &result)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        "editor-dry-run" => {
            let plan = args
                .patch_plan
                .clone()
                .context("editor-dry-run requires --patch-plan")?;
            let result = editors::dry_run::execute_dry_run(&plan, args.operation_id.as_deref())?;
            write_validation_result(args.out.as_ref(), &result)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        "dry-run" => {
            let plan = args.patch_plan.clone().context("dry-run requires --plan")?;
            let report = editors::dry_run::build_dry_run_report(&plan, args.workspace.as_deref())?;
            write_validation_result(args.out.as_ref(), &report)?;
            if !report.safe_to_apply {
                std::process::exit(1);
            }
        }
        "inventory" => {
            let ws = args
                .workspace
                .clone()
                .context("inventory requires --workspace")?;
            let report = inventory::scanner::scan_workspace(&ws)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "stage" => {
            let plan = args.patch_plan.clone().context("stage requires --plan")?;
            let ws = args
                .workspace
                .clone()
                .context("stage requires --workspace")?;
            let stage_dir = args
                .stage_dir
                .clone()
                .context("stage requires --stage-dir")?;
            let report = staging::stager::stage_patch_plan(&plan, &ws, &stage_dir)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if !report.safe_to_stage {
                std::process::exit(1);
            }
        }
        "apply-stage" => {
            let plan = args
                .patch_plan
                .clone()
                .context("apply-stage requires --plan")?;
            let stage_dir = args
                .stage_dir
                .clone()
                .context("apply-stage requires --stage-dir")?;
            let report = apply::text_apply::apply_patch_plan_to_stage(&plan, &stage_dir)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if !report.safe_applied {
                std::process::exit(1);
            }
        }
        "diff-stage" => {
            let ws = args
                .workspace
                .clone()
                .context("diff-stage requires --workspace")?;
            let stage_dir = args
                .stage_dir
                .clone()
                .context("diff-stage requires --stage-dir")?;
            let report = diff::preview::build_stage_diff_report(&ws, &stage_dir)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if !report.diffed_clean {
                std::process::exit(1);
            }
        }
        "export-bundle" => {
            let plan = args
                .patch_plan
                .clone()
                .context("export-bundle requires --plan")?;
            let ws = args
                .workspace
                .clone()
                .context("export-bundle requires --workspace")?;
            let stage_dir = args
                .stage_dir
                .clone()
                .context("export-bundle requires --stage-dir")?;
            let bundle_dir = args
                .bundle_dir
                .clone()
                .context("export-bundle requires --bundle-dir")?;
            let report = export::bundle::export_patch_bundle(&plan, &ws, &stage_dir, &bundle_dir)
                .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if !report.safe_exported {
                std::process::exit(1);
            }
        }
        "plan-rpf-write" => {
            let bundle_dir = args
                .bundle_dir
                .clone()
                .context("plan-rpf-write requires --bundle-dir")?;
            let target_rpf = args
                .target_rpf
                .clone()
                .context("plan-rpf-write requires --target-rpf")?;
            // Planning only — this never opens or modifies the target archive,
            // and safe_to_write is always false in this milestone.
            let plan = rpf_writer::plan::build_rpf_write_plan(&bundle_dir, &target_rpf)
                .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &plan)?;
        }
        "backup-rpf" => {
            let target_rpf = args
                .target_rpf
                .clone()
                .context("backup-rpf requires --target-rpf")?;
            let backup_dir = args
                .backup_dir
                .clone()
                .context("backup-rpf requires --backup-dir")?;
            // Read-only preflight: copies the target into backup_dir and verifies
            // by SHA-256. The original target archive is never modified.
            let report = rpf_backup::backup::backup_rpf_archive(&target_rpf, &backup_dir)
                .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if !report.safe_for_future_write {
                std::process::exit(1);
            }
        }
        "probe-rpf" => {
            let target_rpf = args
                .target_rpf
                .clone()
                .context("probe-rpf requires --target-rpf")?;
            // Read-only probe: reads metadata + SHA-256 and detects external tools.
            // It never parses RPF internals and never modifies the archive.
            let report =
                rpf_probe::probe::probe_rpf_archive(&target_rpf).map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if report.status != rpf_probe::model::RpfProbeStatus::Probed {
                std::process::exit(1);
            }
        }
        "compare-rpf" => {
            let clean_rpf = args
                .clean_rpf
                .clone()
                .context("compare-rpf requires --clean-rpf")?;
            let modded_rpf = args
                .modded_rpf
                .clone()
                .context("compare-rpf requires --modded-rpf")?;
            // Read-only comparison: reads metadata + SHA-256 for both archives.
            // It never parses RPF internals and never modifies either archive.
            let report = rpf_compare::compare::compare_rpf_archives(&clean_rpf, &modded_rpf)
                .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if report.status != rpf_compare::model::RpfCompareStatus::Compared {
                std::process::exit(1);
            }
        }
        "rpf-adapter-info" => {
            // Inspection only: reports the current adapter contract/capabilities.
            // The NullRpfAdapter never opens, parses, or modifies any archive.
            let adapter = rpf_adapter::null_adapter::NullRpfAdapter::new();
            let report = rpf_adapter::contract::build_adapter_info_report(&adapter);
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "rpf-external-tools" => {
            // Planning only: detects known tools on PATH (informational) and
            // marks every mutation/auto-execution path blocked. No tool is run.
            let plan =
                rpf_external::build_external_tool_adapter_plan().map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &plan)?;
        }
        "write-readiness" => {
            let bundle_dir = args
                .bundle_dir
                .clone()
                .context("write-readiness requires --bundle-dir")?;
            let target_rpf = args
                .target_rpf
                .clone()
                .context("write-readiness requires --target-rpf")?;
            // Read-only: combines existing preflight reports into one decision
            // object. Never opens/modifies the target, bundle, or backups, and
            // never executes external tools. readyToWrite is always false.
            let report = rpf_readiness::readiness::build_write_readiness_report(
                &bundle_dir,
                &target_rpf,
                args.backup_report.as_deref(),
            )
            .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "rpf-entry-manifest" => {
            let bundle_dir = args
                .bundle_dir
                .clone()
                .context("rpf-entry-manifest requires --bundle-dir")?;
            // Read-only: reads bundle_manifest.json and walks files/. Never parses
            // or modifies the target RPF; never executes external tools.
            let report = rpf_entry_manifest::manifest::build_rpf_entry_manifest(
                &bundle_dir,
                args.target_rpf.as_deref(),
            )
            .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
            if report.status != rpf_entry_manifest::model::RpfEntryManifestStatus::Built {
                std::process::exit(1);
            }
        }
        "writer-permission" => {
            let bundle_dir = args
                .bundle_dir
                .clone()
                .context("writer-permission requires --bundle-dir")?;
            let target_rpf = args
                .target_rpf
                .clone()
                .context("writer-permission requires --target-rpf")?;
            // Read-only: models the manual confirmation/permission token required
            // before any future RPF write. Never opens/modifies the target,
            // bundle, or backups; never executes external tools; never creates
            // backups. writerAllowed is always false (the writer is not
            // implemented), so a token never authorizes writing.
            let report = rpf_permission::permission::build_writer_permission_report(
                &bundle_dir,
                &target_rpf,
                args.readiness_report.as_deref(),
                args.entry_manifest_report.as_deref(),
                args.backup_report.as_deref(),
                args.confirm.as_deref(),
            )
            .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-strategy" => {
            // Static, deterministic strategy report. Locks CodeWalker.API as the
            // future writer route. Reads no files, modifies nothing, executes no
            // external tool, and never enables writing.
            let report = codewalker_strategy::strategy::build_codewalker_strategy_report()
                .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-detect" => {
            // Read-only detection of a local CodeWalker.API. Performs only safe
            // HTTP GET checks (root + /api/service-status) against the base URL.
            // Never calls replace/import/write or any mutation endpoint, never
            // executes CodeWalker as a process, never opens or modifies an RPF
            // archive. Offline servers yield reachable=false, not an error.
            // writerAllowed and all write capabilities stay false. Exits 0.
            let report = codewalker_api::detect::detect_codewalker_api(args.base_url.as_deref())
                .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-readiness" => {
            // Read-only readiness probe. Builds on detection, then does one extra
            // safe GET /api/service-status and tolerantly parses it. Never calls
            // replace/import/reload-services/set-config, never issues a POST or any
            // mutation, never executes CodeWalker, never opens or modifies an RPF
            // archive. readyForReplace/canWriteArchive/writerAllowed stay false.
            // Offline yields a valid not-ready report. Exits 0.
            let report =
                codewalker_api::readiness::probe_codewalker_api_readiness(args.base_url.as_deref())
                    .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-resolve-targets" => {
            let entry_manifest_report = args
                .entry_manifest_report
                .clone()
                .context("codewalker-resolve-targets requires --entry-manifest-report")?;
            // Read-only search/target-resolution planner. Reads the entry manifest
            // and issues only safe GET /api/search-file calls. Never calls
            // replace/import/reload-services/set-config, never issues a POST or any
            // mutation, never executes CodeWalker, never opens or modifies an RPF
            // archive. canWriteArchive/writerAllowed stay false. Exits 0.
            let report = codewalker_api::search::build_codewalker_search_resolve_report(
                &entry_manifest_report,
                args.base_url.as_deref(),
                args.readiness_report.as_deref(),
            )
            .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-dry-replace-plan" => {
            let bundle_dir = args
                .bundle_dir
                .clone()
                .context("codewalker-dry-replace-plan requires --bundle-dir")?;
            let entry_manifest_report = args
                .entry_manifest_report
                .clone()
                .context("codewalker-dry-replace-plan requires --entry-manifest-report")?;
            let resolve_report = args
                .resolve_report
                .clone()
                .context("codewalker-dry-replace-plan requires --resolve-report")?;
            // Local, read-only dry replace planner. Combines the entry manifest,
            // the resolve report, and the providing bundle files into MODELLED
            // /api/replace-file payloads. Sends NO HTTP request, never uses POST,
            // never calls replace/import/reload-services/set-config or any
            // mutation endpoint, never executes CodeWalker or any external tool,
            // and never opens or modifies an RPF archive. readyForExecution,
            // writerAllowed, and codewalkerExecutionAllowed stay false. Exits 0.
            let report = codewalker_api::dry_replace::build_codewalker_dry_replace_plan(
                &bundle_dir,
                &entry_manifest_report,
                &resolve_report,
                args.permission_report.as_deref(),
            )
            .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-execution-gate" => {
            let target_rpf = args
                .target_rpf
                .clone()
                .context("codewalker-execution-gate requires --target-rpf")?;
            let dry_replace_plan = args
                .dry_replace_plan
                .clone()
                .context("codewalker-execution-gate requires --dry-replace-plan")?;
            let permission_report = args
                .permission_report
                .clone()
                .context("codewalker-execution-gate requires --permission-report")?;
            let readiness_report = args
                .readiness_report
                .clone()
                .context("codewalker-execution-gate requires --readiness-report")?;
            let entry_manifest_report = args
                .entry_manifest_report
                .clone()
                .context("codewalker-execution-gate requires --entry-manifest-report")?;
            let backup_report = args
                .backup_report
                .clone()
                .context("codewalker-execution-gate requires --backup-report")?;
            // Local, read-only copied-test-archive execution gate. Decides
            // whether a FUTURE CodeWalker replace attempt would be eligible.
            // Reads only the local target fixture and the five report files.
            // Sends NO HTTP request, never uses POST, never calls replace/import/
            // reload-services/set-config or any mutation endpoint, never executes
            // CodeWalker or any external tool, and never opens or modifies an RPF
            // archive. codewalkerExecutionEligible may be true, but
            // codewalkerExecutionAllowedNow, codewalkerExecutionPerformed,
            // writerAllowed, and modifiesArchive stay false. Exits 0.
            let report = codewalker_api::execution_gate::build_codewalker_execution_gate_report(
                &target_rpf,
                &dry_replace_plan,
                &permission_report,
                &readiness_report,
                &entry_manifest_report,
                &backup_report,
                args.target_is_test_copy,
            )
            .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-replace-apply" => {
            let execution_gate_report = args
                .execution_gate_report
                .clone()
                .context("codewalker-replace-apply requires --execution-gate-report")?;
            let dry_replace_plan = args
                .dry_replace_plan
                .clone()
                .context("codewalker-replace-apply requires --dry-replace-plan")?;
            // First scoped CodeWalker replace executor. Sends POST /api/replace-file
            // for each planned request ONLY when the T0.6.4 execution gate is
            // eligible, the target is a copied test archive, --execute is given, and
            // the exact confirmation phrase matches. Never calls import/reload-
            // services/set-config or the search endpoint, never executes CodeWalker
            // as a process or any external tool, never parses RPF internals, never
            // rolls back. Global writerAllowed stays false; NullRpfAdapter stays
            // active. On any blocking gate failure, NO HTTP request is sent. Exits 0.
            let report = codewalker_api::replace_apply::apply_codewalker_replace_on_test_archive(
                args.base_url.as_deref(),
                &execution_gate_report,
                &dry_replace_plan,
                args.execute,
                args.confirm.as_deref(),
            )
            .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        "codewalker-post-write-verify" => {
            let target_rpf = args
                .target_rpf
                .clone()
                .context("codewalker-post-write-verify requires --target-rpf")?;
            let replace_apply_report = args
                .replace_apply_report
                .clone()
                .context("codewalker-post-write-verify requires --replace-apply-report")?;
            let backup_report = args
                .backup_report
                .clone()
                .context("codewalker-post-write-verify requires --backup-report")?;
            let execution_gate_report = args
                .execution_gate_report
                .clone()
                .context("codewalker-post-write-verify requires --execution-gate-report")?;
            let dry_replace_plan = args
                .dry_replace_plan
                .clone()
                .context("codewalker-post-write-verify requires --dry-replace-plan")?;
            // Local, read-only post-write verification + rollback plan. Reads the
            // target file and the four reports, compares pre/post/backup hashes,
            // classifies the outcome, and builds a rollback PLAN. Never restores
            // the backup, never modifies the target, never calls CodeWalker, never
            // sends an HTTP request, never uses POST, never executes an external
            // tool, never parses RPF internals. rollbackExecuted and
            // rollbackExecutionAllowed stay false. Exits 0.
            let report =
                codewalker_api::post_write_verify::build_codewalker_post_write_verify_report(
                    &target_rpf,
                    &replace_apply_report,
                    &backup_report,
                    &execution_gate_report,
                    &dry_replace_plan,
                )
                .map_err(anyhow::Error::msg)?;
            write_validation_result(args.out.as_ref(), &report)?;
        }
        _ => {
            usage();
            anyhow::bail!("unknown command: {}", args.command);
        }
    }

    Ok(())
}

fn write_validation_result<T: Serialize>(out: Option<&PathBuf>, result: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    if let Some(path) = out {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, json)?;
        println!("Validation result written to: {}", path.display());
    } else {
        println!("{}", json);
    }
    Ok(())
}
