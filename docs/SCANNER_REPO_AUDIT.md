# Redux Scanner Repo Audit

## Executive Summary

This repository is a two-part scanner: a C++ CLI launcher in `src/cpp/redux_rpf_scanner.cpp` and a Rust backend in `rpf_backend_rs/src/main.rs`. The intended architecture is clear and mostly consistent across `README.md`, `docs/ARCHITECTURE.md`, and the code: the scanner is meant to stay read-only, scan GTA V RPF archives, hash files, compare clean vs modded archives, and emit JSON reports.

Current state is **promising but not release-ready**. The Windows C++ launcher still builds successfully with CMake/MSBuild, but the Rust backend currently fails both `cargo check` and `cargo test` because of active borrow-checker errors in `rpf_backend_rs/src/main.rs`. That means end-to-end `scan-rpf` and `compare-rpf` are effectively blocked in current source form even though the command handlers and report-writing logic exist.

Biggest repo-level findings:

1. **Core scan/compare logic exists** in source: archive walking, hashing, limited nested RPF recursion, diffing, component classification, rules loading, structured warnings, and schema-v2-style metadata.
2. **Current backend does not compile**, with exact failures matching known issues:
   - `E0499`: mutable borrow of `reports` more than once around tracer/hit_effect component reports.
   - `E0382`: partial move of `args.out` before later borrowing `args`.
   - `E0382`: partial move of `args.modded` before later borrowing `args`.
3. **Docs and sample outputs are ahead/behind code at same time**:
   - `README.md` describes schema v2 output.
   - `rpf_backend_rs/src/main.rs` also defines schema-v2-style structs.
   - `examples/sample_outputs/*.json` are still older-style outputs without `schemaVersion`, `tool`, `timing`, `scan`, or `rules`.
4. **Rules support is incomplete at launcher level**: the C++ launcher parses `--component-rules`, `--target-rules`, and `--rules-dir`, but `build_backend_args()` never forwards them to the Rust backend.
5. **Linux intent exists, Linux proof does not**: POSIX code paths and `scripts/build_linux.sh` exist, but there is no CI, no Linux test evidence, and no verified backend build in this audit.

## Current Capabilities

### Repo overview

| Area | Current state | Evidence |
|---|---|---|
| Languages/frameworks | C++17 launcher + Rust 2021 backend | `CMakeLists.txt`, `rpf_backend_rs/Cargo.toml` |
| Main binaries/tools | `redux_rpf_scanner` / `redux_rpf_scanner.exe`, `rpf_backend_rs` / `rpf_backend_rs.exe` | `README.md`, `scripts/build_windows.ps1`, `scripts/build_linux.sh` |
| Important folders | `src/cpp`, `rpf_backend_rs`, `rules`, `docs`, `examples/sample_outputs`, `scripts` | repo layout |
| Finished-looking parts | CLI wrapper shape, argument parsing, version command, validate-tools command, scan/compare report models, hash/diff logic, Windows launcher build | `src/cpp/redux_rpf_scanner.cpp`, `rpf_backend_rs/src/main.rs`, successful CMake build |
| Experimental/broken parts | Rust backend compile, schema/sample alignment, rules forwarding from launcher, Linux readiness, cache/fingerprint/deep analyzers | build/test failures, missing code, stale samples |

### Capability matrix

| Capability | Status | Notes |
|---|---|---|
| `scan-rpf` | **broken** | Command exists in C++ and Rust, but backend does not currently compile. |
| `compare-rpf` | **broken** | Command exists in C++ and Rust, but backend does not currently compile. |
| Recursive/nested RPF scan | **partial** | `scan_archive_inner()` recurses only into selected nested RPF targets via `is_nested_rpf_target()` (`scaleform_minimap.rpf`, `ptfx.rpf`). Not generic nested full-tree scanning. |
| Hashing files | **exists** | `sha256_hex()` used during scan; compare always hashes, scan hashes except `fast` mode. |
| Outputting JSON | **exists** | Launcher `validate-tools` prints JSON; Rust scan/compare code writes JSON manifests/reports. End-to-end blocked by backend build. |
| Outputting Markdown | **missing** | No Markdown emitter in launcher or backend. |
| Classifying files/components | **partial** | Heuristic classification + optional rules support exist, but only for limited components/categories. |
| Rules/config files | **partial** | Rust loads rules files and has fallback logic; launcher parses rule flags but does not forward them. |
| Cache | **missing** | No cache structures, files, or CLI. |
| Tool/backend validation | **partial** | Launcher `validate-tools` checks backend path and key file presence, plus backend `version` output if backend exists. No backend self-test, no archive open test. |
| Scanner `version` command | **partial** | Launcher version works; backend version command exists in source but backend build is broken. |
| Linux build | **partial** | POSIX launcher path and Linux build script exist; no verification, no CI, backend not proven. |
| Reading GTA/RPF key files | **partial** | Rust calls `GtaKeys::load_from_path`; launcher validates file presence. Blocked in practice by backend compile failure. |
| Error/warning reporting | **partial** | Structured warnings exist in Rust output model; hard failures still mostly bubble as CLI stderr/errors, not structured report errors. |

## Build And Test Status

### How to build

Documented build paths:

- **Rust backend:** `cargo build --release` inside `rpf_backend_rs`
- **C++ launcher:** `cmake -S . -B build` then `cmake --build build --config Release`
- **Packaged layout helpers:** `scripts/build_windows.ps1` and `scripts/build_linux.sh`

Important detail: `CMakeLists.txt` only builds the C++ launcher. It does **not** build/package the Rust backend or create the `dist/tools` layout by itself. Packaging currently lives in helper scripts, not in CMake.

### Dependencies

From docs and build scripts:

- C++17 compiler (`MSVC` or `g++`)
- CMake 3.20+
- Rust stable + Cargo
- Rust crates from `rpf_backend_rs/Cargo.toml`: `rpf-archive`, `serde`, `serde_json`, `sha2`, `hex`, `tempfile`, `anyhow`, `time`

### Tests found

Only three unit tests were found, all in `rpf_backend_rs/src/main.rs`:

- `parse_scan_mode_accepts_valid_values`
- `parse_scan_mode_rejects_invalid_values`
- `resolve_scan_mode_defaults_to_targeted`

No integration tests were found for:

- opening real or fixture RPF archives
- compare behavior
- nested RPF recursion
- rules loading
- structured warnings
- C++ launcher argument forwarding

No `ctest`, no separate Rust `tests/` directory, no GitHub Actions workflows.

### Commands run during this audit

| Command | Result | Notes |
|---|---|---|
| `cmake -S . -B build` | **passed** | Generated Visual Studio build files on Windows. |
| `cmake --build build --config Release` | **passed** | Built `redux_rpf_scanner.exe` successfully. |
| `cargo check --manifest-path rpf_backend_rs\Cargo.toml` | **failed** | Rust backend compile blocked by 4 errors. |
| `cargo test --manifest-path rpf_backend_rs\Cargo.toml` | **failed** | Same compile blockers as `cargo check`. |
| `.\build\Release\redux_rpf_scanner.exe version` | **passed** | Printed `redux_rpf_scanner 0.2.0`. |
| `.\build\Release\redux_rpf_scanner.exe validate-tools --keys <repo-root>` | **passed as expected failure** | Printed JSON validation result; backend missing in default packaged location, key files missing. |

### Exact failing commands and relevant error summary

#### `cargo check --manifest-path rpf_backend_rs\Cargo.toml`

Failed with:

```text
error[E0499]: cannot borrow `*reports` as mutable more than once at a time
  --> src\main.rs:1574:43

error[E0499]: cannot borrow `*reports` as mutable more than once at a time
  --> src\main.rs:1591:43

error[E0382]: borrow of partially moved value: `args`
  --> src\main.rs:1823:36
  note: `args.out` partially moved due to this method call

error[E0382]: borrow of partially moved value: `args`
  --> src\main.rs:1877:36
  note: `args.modded` partially moved due to this method call
```

This exactly matches known current issues.

#### `cargo test --manifest-path rpf_backend_rs\Cargo.toml`

Failed with the same four errors:

```text
error[E0499]: cannot borrow `*reports` as mutable more than once at a time
  --> src\main.rs:1574:43

error[E0499]: cannot borrow `*reports` as mutable more than once at a time
  --> src\main.rs:1591:43

error[E0382]: borrow of partially moved value: `args`
  --> src\main.rs:1823:36
  note: `args.out` partially moved due to this method call

error[E0382]: borrow of partially moved value: `args`
  --> src\main.rs:1877:36
  note: `args.modded` partially moved due to this method call
```

### Windows build status

- **Launcher on Windows:** confirmed working in this audit.
- **Full scanner on Windows:** not currently working from source because Rust backend fails to compile.
- **Packaged Windows flow:** likely intended through `scripts/build_windows.ps1`, but unverified because backend build failed.

### Linux / Ubuntu readiness

Linux intent exists:

- POSIX code path in `redux_rpf_scanner.cpp`
- `scripts/build_linux.sh`
- Linux requirements in `README.md`

But Linux is **not ready to claim as supported** yet because:

- no Linux CI
- no Linux build/test evidence in repo
- backend currently fails to compile before platform-specific validation even matters
- no packaging/install docs beyond a basic shell script

### Missing build docs

1. No note that `CMakeLists.txt` builds only launcher, not full packaged scanner.
2. No verified end-to-end Windows build instructions after current Rust failures.
3. No Linux validation matrix or tested distro/version.
4. No CI workflow docs or reproducible release packaging flow.
5. No fixture/test archive strategy documented.

## Existing Commands

### Confirmed commands in launcher

From `src/cpp/redux_rpf_scanner.cpp` and runtime help:

- `scan-rpf`
- `compare-rpf`
- `version`
- `validate-tools`

Supported flags exposed by launcher:

- `--clean`
- `--modded`
- `--archive`
- `--keys`
- `--out`
- `--backend`
- `--depth`
- `--mode fast|targeted|deep|full`
- deprecated `--all`
- deprecated `--targets-only`
- parsed but not actually forwarded: `--component-rules`, `--target-rules`, `--rules-dir`

### Confirmed commands in backend source

From `rpf_backend_rs/src/main.rs`:

- `scan`
- `compare`
- `version`

No backend `validate-tools` command exists.

### Important command mismatch

`src/cpp/redux_rpf_scanner.cpp` parses rule arguments into `Args`, but `build_backend_args()` never appends:

- `--component-rules`
- `--target-rules`
- `--rules-dir`

So rules-file support is implemented in Rust, advertised by launcher parsing/help, but **not actually reachable through normal launcher execution**.

## Current Output Format

### Current source-defined structure

Rust source defines schema-v2-style output:

- `schemaVersion`
- `tool`
- `timing`
- `scan`
- `rules`
- `warnings`
- compare report with `components` and `allChanges`
- scan manifest with `files`

Key functions/structs:

- `Report`
- `write_scan_manifest()`
- `build_tool_metadata()`
- `build_scan_metadata()`
- `load_rules()`
- `build_rich_metadata()`

### Current example output files

Shipped samples in `examples/sample_outputs/` are older and do **not** match current source models:

| File | Observed shape | Audit note |
|---|---|---|
| `deep_manifest.json` | has `ok`, `backend`, `archive`, `keysPath`, `depth`, `stats`, `warnings`, `files` | missing `schemaVersion`, `tool`, `timing`, `scan`, `rules` |
| `component_diff.json` | has `ok`, `backend`, `cleanInput`, `moddedInput`, `keysPath`, `depth`, `stats`, `warnings`, `components`, `allChanges` | missing `schemaVersion`, `tool`, `timing`, `scan`, `rules`; also older per-file shape |

### Output/report structure review

| Question | Answer | Notes |
|---|---|---|
| Are reports stable and structured? | **partial** | Structs are structured, but backend build broken and samples stale. |
| Do they include `schemaVersion`? | **source: yes / samples: no** | `SCHEMA_VERSION = "2.0"` in Rust, but sample outputs are pre-v2. |
| Do they include scanner version? | **source: yes / samples: no** | Via `tool.version`; launcher passes `--scanner-version`. |
| Do they include archive hash/size? | **missing** | No archive identity metadata in manifest/report structs. |
| Do they include timing? | **source: yes / samples: no** | `Timing` struct exists; stale samples do not show it. |
| Do they include warnings/errors? | **warnings: partial / errors: missing** | Structured warnings exist; no structured report-level `errors` array for failed scans. |
| Are paths normalized? | **partial** | Internal entry paths are normalized to lowercase `/`; top-level input/source paths remain platform-native absolute paths. |
| Is JSON AI-readable? | **partial** | Pretty JSON, deterministic fields, hashes, categories. But no archive identity, no unknown queue, duplicate component hits, stale samples. |
| Is output useful for HomeOps jobs/UI? | **partial** | Useful for manual inspection; not yet strong enough for cached baselines, schema migration, or UI contracts. |
| Is schema versioned well enough for future changes? | **partial** | Version constant exists, but no migration docs/tests and shipped samples contradict current schema claims. |

### Specific format issues

1. **No archive hash/size metadata** for baseline identity or cache keys.
2. **No distinct tree/fingerprint output** separate from scan manifest.
3. **No structured error array** on hard failure.
4. **Duplicate component evidence is possible**. `apply_fallback_classification()` can add same file multiple times to multiple reports for heuristic reasons.
5. **Absolute local lab paths are embedded in sample outputs**, which is noisy for docs and not ideal for shared examples.
6. `component_diff.json` sample shows `cleanEntries: 0` and `changedEntries: 284`, which makes it a weak baseline example for expected compare behavior.

## Missing Features

Major missing or incomplete areas relative to target repo purpose:

- stable end-to-end build of Rust backend
- generic full nested RPF tree scanning
- baseline tree output and fingerprint output
- cache
- compare-against-cached-baseline flow
- unknown pattern queues and review artifacts
- deep inside-file analyzers
- Markdown report generation
- Linux verification/CI
- HomeOps-oriented stable job artifacts
- AI corpus generation pipeline

## Safety Findings

| Area | Finding | Risk |
|---|---|---|
| Archive mutation | No archive writing code found. Scanner stays read-only in current source. | **good** |
| Writes outside output dir | Writes occur to user-selected report path and system temp for nested RPF temp files. No random extra writes found. | **low** |
| Input overwrite | No code writes back to input archives or keys paths. | **good** |
| File deletion | No destructive deletion logic found. `TempDir` handles temp cleanup. | **good** |
| Hardcoded absolute paths | Present in `README.md` examples and sample outputs (`D:\AI_Redux_Lab`, `C:\Users\Marcel\Downloads\rpf_keys`). | **low/cleanup needed** |
| Secrets/keys committed | No actual key files found. Only filenames are referenced. | **good** |
| Proprietary assets tracked | No tracked `.rpf`, `.ytd`, `.ypt`, `.gfx`, `.dat` asset files found via `git ls-files`. | **good** |
| Large tracked files | Largest tracked files are JSON samples (`component_diff.json` ~222.6 KB, `deep_manifest.json` ~67.6 KB). | **low** |
| Temp handling | Nested RPF extraction writes temp files via `TempDir::new()`. Safe enough, but no size cap or temp storage accounting. | **medium** |
| Error handling | Mostly explicit; warnings structured in Rust. Hard failures still return CLI errors, not structured report artifacts. | **medium** |
| Unsafe path assumptions | User-provided output path is trusted and parent dirs are auto-created. Expected, but no policy guardrails. | **low** |
| Commands that could modify archives | None found. | **good** |
| Copyright/raw asset risk | Current repo only tracks derived JSON samples. Future extracted text/assets must remain local-only. | **important future constraint** |

Additional safety note: `validate-tools` only checks key file presence, not whether keys are valid or whether the backend can actually open a real archive. That is safe, but shallow.

## Full Baseline Scan Readiness

Target workflow:

```text
clean update.rpf
→ full internal scan of everything
→ full_clean_manifest.json
→ full_clean_tree.json
→ baseline_update_tree_fingerprint.json
→ cached baseline metadata
```

| Workflow step | Status | Notes |
|---|---|---|
| Accept clean `update.rpf` input | **exists** | CLI/source support present. |
| Full internal scan of everything | **partial** | `--mode full` exists, but nested recursion is still selective, not generic. Backend build currently blocks use. |
| `full_clean_manifest.json` | **partial** | `scan` can write a manifest JSON, but there is no baseline-specific naming or contract. |
| `full_clean_tree.json` | **missing** | No dedicated tree output separate from manifest. |
| `baseline_update_tree_fingerprint.json` | **missing** | No fingerprint model or command. |
| Cached baseline metadata | **missing** | No cache. |

### What already exists

- `scan` / `scan-rpf` command shape
- file hashing
- normalized entry paths
- limited nested RPF scan
- JSON manifest writer
- scan mode abstraction

### What is partial

- full-scan semantics: current `full` mode still depends on `is_nested_rpf_target()` for nested traversal
- schema output: defined in source, not reflected in sample outputs
- rules-based targeting: Rust side exists, launcher-side wiring incomplete

### What is missing

- canonical baseline artifact set
- tree-only output
- archive identity metadata
- fingerprint output
- baseline cache index
- acceptance tests around a clean baseline workflow

### What is blocked by current bugs

- all backend-driven baseline generation is blocked by current Rust compile failures

### What should be implemented first

1. Fix backend compile blockers.
2. Lock down a single stable manifest/tree schema.
3. Add archive identity metadata (`archiveSize`, `archiveSha256`, maybe entry count + tree fingerprint seed).
4. Add separate tree/fingerprint outputs before cache.

## Modded Scan / Diff Readiness

Target workflow:

```text
modded update.rpf
→ full internal scan
→ full_modded_manifest.json
→ compare against cached clean baseline
→ detect every added/removed/modified file
```

| Workflow step | Status | Notes |
|---|---|---|
| Accept modded archive input | **exists** | CLI/source support present. |
| Full internal scan | **partial** | Same limitations as baseline scan. |
| `full_modded_manifest.json` | **partial** | Scan manifest writer exists, but no named contract for modded baseline artifacts. |
| Compare against clean archive directly | **exists in source / broken in repo** | `diff_maps()` and compare command exist, but backend compile failure blocks use. |
| Compare against cached clean baseline | **missing** | Compare requires live clean + modded archives, not cached metadata. |
| Detect added/removed/modified files | **exists in source / broken in repo** | `diff_maps()` handles all three states. |

### What already exists

- direct clean-vs-modded compare model
- `allChanges`
- `components`
- file-level sha256 diffing
- `added` / `removed` / `modified` statuses

### What is partial

- full scan still not truly full nested tree scan
- component classification is heuristic and limited
- rules support is partly stranded behind launcher forwarding gap

### What is missing

- compare against cached baseline manifest/tree
- separate full modded manifest contract
- full tree diff artifacts for HomeOps jobs
- archive-level metadata for trustable baseline matching

### What is blocked by current bugs

- backend compile failures stop any real compare run from current source

### What should be implemented first

1. Fix backend compile.
2. Decide baseline cache format.
3. Add compare mode that accepts cached baseline metadata/tree instead of raw clean archive only.
4. Add tests proving full added/removed/modified coverage.

## Unknown Pattern Discovery Readiness

| Need | Status | Notes |
|---|---|---|
| Preserve unknown changed files | **partial** | In `full` mode, unknown files can still land in `allChanges`; in targeted mode, unknown files can be filtered out before diff. |
| Classify unknown text vs unknown binary | **missing** | Current fallback is mostly `unknown_binary` / `unknown` component. No text/binary sniffing. |
| Inspect unknown readable files | **missing** | No file-content analyzers beyond hashing. |
| Create `unknown_changes.json` | **missing** | No dedicated artifact. |
| Create `candidate_patterns.json` | **missing** | No rule-suggestion or pattern-mining output. |
| Create LLM review queue | **missing** | No queue/build artifact. |
| Avoid ignoring files just because they are not known rules | **partial** | Possible in `full` mode; not true in targeted mode because targeting intentionally filters. |

### Current reality

The repo can preserve unknown files only when scan mode includes them. It cannot yet:

- separate unknown readable text from opaque binary
- extract candidate signatures
- build review queues
- generate follow-up artifacts for future rules authoring

This area is still mostly conceptual, not implemented.

## Tree Fingerprint / Renamed Update RPF Readiness

Target workflow:

```text
clean update.rpf
→ baseline tree fingerprint

unknown .rpf not named update
→ quick tree scan
→ compare to baseline update tree
→ classify as:
  obvious_update_rpf
  likely_update_rpf
  possible_update_rpf
  not_update_rpf
  unknown_rpf
  scan_failed
```

### Current status

**Missing.** No fingerprint data model, no command, no archive-identity classifier, no quick tree-only mode dedicated to archive type detection.

### What exists that could support it later

- normalized path generation
- entry counting
- hashing helper
- scan mode abstraction

### What is missing

- baseline tree fingerprint schema
- fast tree-only scan path that does not require full content hashing
- similarity scoring logic
- classification thresholds
- CLI command or output contract

### Data structures likely needed

1. Sorted normalized path list
2. Entry count + extension histogram
3. Selected anchor paths set
4. Optional path+size signature map
5. Stable tree fingerprint hash over normalized path list / signature tuples
6. Match report with score, missing anchors, extra anchors, confidence bucket

### CLI command or mode that would make sense

Best fit is probably a **new command**, not an overloaded scan mode. Example:

```text
redux_rpf_scanner fingerprint-rpf --archive <archive.rpf> --keys <keys_dir> --out <fingerprint.json>
redux_rpf_scanner classify-rpf --archive <archive.rpf> --baseline <baseline_update_tree_fingerprint.json> --keys <keys_dir> --out <classification.json>
```

### Acceptance criteria

1. Correctly identifies known clean `update.rpf` as `obvious_update_rpf`.
2. Correctly rejects unrelated RPFs as `not_update_rpf`.
3. Produces deterministic score and fingerprint across repeated runs.
4. Works without full deep extract/analyzer logic.
5. Emits reasons, not just label.

## Deep Analyzer Readiness

Current scanner does **classification only**, not true inside-file analysis. `build_rich_metadata()` identifies likely file families by path/extension, but no analyzers parse file internals yet.

| Format/family | Current state | Difficulty | Priority | Recommended first implementation | Risks |
|---|---|---:|---:|---|---|
| XML / timecycle | **missing** | Low-Medium | 1 | Extract readable XML safely, parse nodes/values, diff clean vs modded, emit AI-friendly JSON | malformed XML, variant schemas, copyright/local-only handling |
| DAT files | **missing** | Medium | 2 | Start with line/key-value diff for known text DATs like `bloodfx.dat` | format inconsistency, binary/text ambiguity |
| META files | **missing** | Medium | 2 | XML-like or structured-text parsing depending on actual file shape | multiple META dialects |
| YMT / YMAP / YTYP if readable | **missing** | Medium-High | 3 | Begin with discoverability/readability probe + metadata summary before deep parsing | many may not be plain text; format/tooling complexity |
| YTD texture dictionaries | **missing** | High | 4 | Metadata-only analyzer first: texture names, counts, dimensions if possible | tooling dependencies, texture decode complexity |
| GFX / SWF | **missing** | High | 5 | Inventory/decompile wrapper only, no editing | external tools, legal/copyright handling, fragile decompilers |
| YPT particle files | **missing** | Very High | 6 | Metadata/index-level analyzer only after XML/DAT/META pipeline mature | hardest format, heavy tooling, noisy outputs |

### Recommended first analyzer

**XML / timecycle**. Repo docs already point here (`docs/DEEP_ANALYZER_PLAN.md`), risk is lower, files are more likely to be readable, and output is directly useful for AI planning later.

## AI Learning Corpus Readiness

| Target artifact | Status | Notes |
|---|---|---|
| `extracted_text/` | **missing** | No extraction pipeline. |
| `learning_corpus/` | **missing** | No corpus builder. |
| `ai_chunks/` | **missing** | No chunker/indexer. |

---

# Cross-Project Audit — R0.8.1 Era

**Date of audit:** 2026-05-28  
**Scanner phase:** R0.8.1 complete  
**OpenRouter Model Tester:** audited alongside scanner  
**Auditor:** GitHub Copilot CLI

This section covers both projects as of the completion of R0.8.1 (timecycle intelligence reports), and evaluates readiness for the next phase: `patch_plan.json` schema, plan validator, and deterministic editor tools.

---

## 1. Overall Verdicts

| Project | Verdict |
|---|---|
| Redux Scanner Engine | **Almost ready** — R0.8.1 complete, reports are AI-usable. One known build gap (Rust binary not auto-copied). |
| OpenRouter Model Tester | **Almost ready** — clean architecture, good safety controls. Rule checker has specific false-positive/false-negative edge cases to fix. |
| Combined AI testing workflow | **Almost ready** — scanner output drives tester input correctly. Workflow gap is rule checker quality + no `patch_plan.json` schema yet. |

---

## 2. Redux Scanner Engine — Current State (R0.8.1)

### What is now complete

All phases R0.1–R0.9 are complete and committed. The scanner can:

- `scan-rpf` — full entry scan, hashing, JSON output, schemaVersion, timing
- `compare-rpf` — diff clean vs modded, per-file change classification, component detection, structured warnings
- `baseline-scan` — writes `baseline_fingerprint.json` with archive hash and entry list
- `diff-against-baseline` — loads baseline, compares modded, emits full diff JSON + Markdown
- `--classify-rpf` — renames renamed/repackaged `update.rpf` detection
- `--analyze-text` — XML/DAT/META internal content analysis (key extraction, diff values, color-like detection)
- `--build-learning-corpus` — aggregates analysis output into structured `learning_corpus/` folder
- `--analyze-text --build-learning-corpus` together → auto-generates `timecycle_intelligence/` subfolder (R0.8.1)
- Batch scanning of multiple Reduxes

### R0.8.1 timecycle intelligence outputs (per Redux)

Each Redux diff output folder gets `timecycle_intelligence/` with:

```
timecycle_intelligence/
├── timecycle_strategy_report.md        — human/AI-readable strategy overview
├── timecycle_file_rankings.json        — ranked candidate files with confidence/risk
├── timecycle_safe_edit_matrix.json     — allowed/blocked/deferred operations per file
├── visualsettings_key_report.json      — named key families, risk levels
├── cloudkeyframes_report.json          — color-only vs numeric evidence
├── weather_xml_report.json             — per-weather and global weather file summary
├── risky_files_report.json             — files AI must not edit first
├── ai_timecycle_context_compact.md     — paste-ready compact AI context (~1000-1800 words)
└── ai_timecycle_prompt_pack.md         — ready-to-use AI prompts
```

Aggregate reports also generated for multi-Redux batch runs in `ai_review_bundle/timecycle_intelligence_aggregate/`.

### Key real-data findings from 3-Redux batch test

| File/family | redux_001 | redux_002 | redux_003 | Recommendation |
|---|---|---|---|---|
| `visualsettings.dat` | ✅ 208 named keys | ✅ 870 numeric keys | ✅ near-full replacement | first_patch (named-key edits only) |
| `cloudkeyframes.xml` | ✅ 1306 numeric + 2075 color | ✗ not present | ✅ 0 numeric + 1322 color (color-ONLY) | first_patch for color operations; color-only pattern confirmed |
| `w_*.xml` family | ✅ 16-17 files changed | ✅ | ✅ pure color-only | first_patch for w_foggy/w_clouds |
| `timecycle_mods_1.xml` | ✅ | ✅ | ✅ | first_patch |
| `timecycle_mods_3.xml` | ~2951 numeric | ~2951 numeric | | blocked — mass numeric risk |
| `weather.xml` | 272 numeric | 14 numeric | 294 numeric | deferred — likely global/system |
| Binary files (.ytd/.ypt/.gfx) | various | various | various | blocked |
| `sky_timecycle` component | ✅ detected | ✅ detected | ✅ detected | confirmed in all 3 |

### Known build workflow gap

**Critical:** `cmake --build build --config Release` rebuilds only the C++ launcher. The Rust backend binary must be **manually copied** after `cargo build --release`:

```powershell
Copy-Item rpf_backend_rs\target\release\rpf_backend_rs.exe build\Release\tools\rpf_backend_rs.exe -Force
```

This caused timecycle_intelligence to silently not generate during early R0.8.1 testing (stale binary was pre-R0.8.1). This gap should be documented in the build scripts or scripted into `build_windows.ps1`.

### Rust backend test count

- **69 tests** pass as of R0.8.1 (`cargo test`)
- 10 new tests added for timecycle intelligence logic
- No test failures

### Scanner report quality for AI testing

| Quality dimension | Assessment |
|---|---|
| Compact enough for free/weak models | ✅ `ai_timecycle_context_compact.md` targets 1000-1800 words |
| Clearly separates first_patch/deferred/blocked | ✅ explicit categories in all JSON outputs |
| visualsettings.dat without overclaiming | ✅ named keys only, no invented meanings |
| cloudkeyframes.xml without overclaiming | ✅ color-only pattern noted as hypothesis/evidence |
| weather.xml safe | ✅ deferred in all 3 Reduxes |
| timecycle_mods_3.xml warned | ✅ blocked for mass numeric edits |
| AI-ready for patch_plan.json | ⚠️ Almost — schema not defined yet; reports provide the right inputs |
| No keys/RPFs in committed examples | ✅ only sanitized fake data in `examples/sample_outputs/` |

---

## 3. OpenRouter Model Tester — Architecture

**Path:** `C:\Users\Marcel\Downloads\openrouter-model-tester`

### Separation quality

| Layer | File | Clean? |
|---|---|---|
| Types | `src/types.ts` | ✅ all interfaces centralized |
| Constants | `src/constants.ts` | ✅ model lists, budget, defaults centralized |
| Rule checks | `src/lib/ruleChecker.ts` | ✅ pure function, no side effects |
| Prompt presets | `src/lib/promptPresets.ts` | ✅ fully declarative, 6 presets + split tasks |
| Backend/API proxy | `server/index.ts` | ✅ Express server, key backend-only |
| Auto eval | `src/components/AutoEvalPanel.tsx` | ✅ clean pipeline: planner → rule check → critic → revision → verdict |
| Main UI | `src/App.tsx` | ✅ state management clean; provider/model/budget logic clear |

**Architecture verdict: clean and extensible.** All key concerns are separated. Adding a new prompt preset, rule, or model requires touching only one file.

---

## 4. Safety

| Safety check | Result |
|---|---|
| OpenRouter API key is backend-only | ✅ `server/index.ts` reads `process.env.OPENROUTER_API_KEY` only; never sent to frontend |
| Paid model call requires confirmation | ✅ `setShowPaidConfirm(true)` gate in `App.tsx:handleRun`; cost estimate shown before confirming |
| Free model failure does NOT silently switch to paid | ✅ free model failures set error + cooldown; paid path is explicit `modelMode === 'paid'` branch |
| Budget cap enforced | ✅ `hardStopWhenBudgetExceeded: true`; per-request limit `$0.10`; session budget `$2.00` |
| No patching/editing behavior in tester | ✅ no RPF write, no file edit, no scanner calls; pure model query + rule check |
| Context files only sent as text | ✅ `FileReader.readAsText()` + appended to `contextText`; no binary upload path |
| Committed examples contain no keys/RPFs/assets | ✅ examples use synthetic/sanitized data |
| Auto Eval paid confirmation before critic/revision | ✅ `if (hasPaid && settings.requirePaidConfirmation) → setShowConfirm(true)` |
| No infinite loops in Auto Eval | ✅ max 1 revision attempt (`maxRevisionAttempts: 1`); critic-after-revision requires `revisedAnswer` to be non-empty |
| Budget tracked across entire session | ✅ `usedBudgetUsd` accumulates in `App.tsx`; passed down to `AutoEvalPanel` via `onBudgetUsed` |

**Safety verdict: strong.** No obvious paths to accidental paid calls or budget blowout.

---

## 5. Model Selection

| Check | Result |
|---|---|
| Free models fetched dynamically from OpenRouter | ✅ `/api/models/free` endpoint filters by `:free` suffix or zero-price |
| Fallback free list used on fetch failure | ✅ `FALLBACK_FREE_MODELS` in constants.ts |
| Paid models are a closed approved list | ✅ `PAID_MODELS` = 4 approved models only |
| Custom model ID path exists | ✅ `__custom__` sentinel + text input |
| deepseek free vs paid variant distinction | ⚠️ `FALLBACK_FREE_MODELS` includes `deepseek/deepseek-v4-flash:free`; `PAID_MODELS` includes `deepseek/deepseek-v4-flash` (no suffix). This is technically correct but may confuse users who see both. Recommend adding a UI note or distinct label. |
| Auto eval uses separate model dropdowns for planner/critic | ✅ separate `plannerModel`/`criticModel` settings |

---

## 6. Prompt Presets Quality

| Preset | Quality |
|---|---|
| `timecycle-plan` (full) | ✅ strong — 7 sections, cautious language, explicit validation requirement, tools section |
| `compact-timecycle` (free models) | ✅ good — 5 sections, 1200-word limit, repeat no-XML/no-values rules |
| `json-patch-plan` | ✅ strict — explicit JSON shape, `unsupportedClaims` field, binary blocked rule |
| `grade-answer` (critic) | ✅ strong — 7 criteria, verdict enum, `shouldRevise`, `hardFails` array |
| `free-model-test` | ✅ appropriate — 5 targeted questions, 600-word limit |
| `paid-model-full` | ✅ comprehensive — 7 full sections + signal quality assessment |
| Split task prompts (5 parts) | ✅ well-separated; useful for models that cut off |
| CONTINUE_PROMPT | ✅ clean; no-repeat rule present |
| REVISION_PROMPT_TEMPLATE | ✅ length-constraint rule included |

**Notable gap:** `recommendedFiles` in all presets references the old corpus file names (`timecycle_ai_prompt_context.md`, `timecycle_focus_summary.md`, etc.). The R0.8.1 actual output file is `ai_timecycle_context_compact.md`. These recommended-file hints should be updated to match R0.8.1 output file names.

Current `recommendedFiles`:
```
timecycle_ai_prompt_context.md       ← old name
timecycle_focus_summary.md           ← old name
timecycle_candidate_files.json       ← old name
timecycle_value_change_summary.json  ← old name
timecycle_patch_planning_questions.md ← old name
```

R0.8.1 actual files:
```
ai_timecycle_context_compact.md      ← primary compact context
timecycle_file_rankings.json
visualsettings_key_report.json
cloudkeyframes_report.json
weather_xml_report.json
```

---

## 7. Auto Planner + Critic Flow

The pipeline in `AutoEvalPanel.tsx` is:

```
1. PLANNER call → plannerAnswer
2. RULE CHECKS (optional) → hard fail or pass with warnings
3. AI CRITIC call (optional) → score /70, verdict, shouldRevise
4. REVISION (optional, only if critic score < acceptThreshold) → revisedAnswer
5. CRITIC AFTER REVISION (optional) → score revised answer
6. FINAL VERDICT → accepted / usable_with_validator / rejected
```

| Flow check | Result |
|---|---|
| Planner failure stops pipeline immediately | ✅ `skipAll(STEP.RULE_CHECK)` called on planner fail |
| Rule check hard fail stops pipeline | ✅ returns immediately after hard fail |
| Rule check runs ALL checks (no early return per check) | ✅ all hard-fail checks run; all hard fails collected |
| Critic score used for final decision | ✅ `avoidsHallucination < minHallucinationScore` → rejected |
| Revision only runs when needed | ✅ `needsRevision = shouldRevise || total < acceptThreshold` |
| Max revisions = 1 | ✅ `maxRevisionAttempts: 1` in defaults; no retry loop in code |
| Budget tracked per step | ✅ `onBudgetUsed(cost)` called after each paid call |
| Cost check before run | ✅ `if (totalCost > settings.maxCostPerRunUsd)` → alert |
| Save as JSON or Markdown | ✅ both supported via `handleSave(fmt)` |
| Paid confirmation before full run | ✅ confirmed |
| Critic cost estimate uses `5000` char overhead for context | ⚠️ estimate = `plannerInputLen + 5000` — this is a rough guess. Real critic context = plannerAnswer (could be 5000-15000 chars). May under-estimate cost for large paid-model planner outputs. Non-critical but worth noting. |

---

## 8. Rule Checker — False Positive / False Negative Analysis

This is the most important area for fixing before trusting Auto Eval results.

### Hard fails — known issues

#### 1. `no_binary_editing` — **LOW false-positive risk** (improved in recent version)

Current behavior: per-sentence safe-phrase bypass. If the sentence containing a binary ext also contains a safe phrase (blocked, deferred, avoid, etc.), the rule is skipped.

**Remaining edge case:** A model that writes `"We should avoid editing .ytd files and instead edit .xml files"` — the safe phrase `avoid` is present on the same sentence as `.ytd`, so no hard fail fires. ✅ This is correct behavior.

**Edge case that could still false-positive:** A model that writes a section header like `"Blocked binary files: .ytd, .ypt"` followed by a separate sentence `"The .ytd format stores textures."` — the second sentence has `.ytd` without a safe phrase and without a dangerous verb. The `DANGEROUS_VERB_RE` check requires an editing verb, so this **should not** fire. ✅

**Verdict: No significant false-positive risk here.**

#### 2. `no_weather_xml_first` — **MEDIUM false-positive risk**

Current behavior: line-by-line check with 3-line section context lookback.

**Confirmed false-positive case:** A model that correctly writes a section like:

```
### Files to Avoid
- weather.xml: deferred, global system file
```

This fires correctly (safe phrase `deferred` is present). ✅

**Problematic case:** A model that writes a first-patch section as a numbered list with file paths, then a separate deferred section:

```
### Recommended First Patch
1. visualsettings.dat
2. cloudkeyframes.xml

### Deferred Files
- weather.xml (global — risky)
```

Here `weather.xml` appears under `Deferred Files` — safe phrases `deferred` and `risky` appear on the same line. ✅ This works.

**Real false-positive risk:** If a model writes weather.xml in a "Detected Changes" or "Scanner findings" context — e.g., `"weather.xml was modified in all 3 Reduxes (272 numeric changes)"` — without saying it's deferred or risky. The line has no safe phrase, no first-patch keyword, and no dangerous verb. The `FIRST_PATCH_KWORDS` check requires a first-patch keyword on the line or in preceding 3 lines. This **should not** fire. ✅

**True false-positive case:** A model writes:

```
## Analysis

scanner detected changes in:
- weather.xml: 272 numeric changes (context note — not first patch target)
```

The phrase "not first patch" is present but uses short form. The `SAFE_PHRASES` list includes `'not first'` — ✅ this should be caught.

**Real gap found:** The `FIRST_PATCH_KWORDS` list includes `'target files'`. A model that writes `"Target files for analysis: weather.xml, visualsettings.dat"` in an analysis-only (non-patch) section could potentially trigger a false positive if the section context contains `target files` AND `weather.xml`. The intent may be "these are files the scanner targeted", not "these are files to patch first". **Recommend: remove `'target files'` from `FIRST_PATCH_KWORDS` or replace with `'target files for first patch'`.**

#### 3. `no_mass_timecycle_edits` — **LOW false-positive risk**

Only fires on specific regex patterns for mass numeric edits. A model correctly warning about mass edits does not trigger this. ✅

#### 4. `requires_validation_plan` — **LOW false-positive risk**

Only checks for `validation`, `verify`, `check after`, `validate`. Any model that mentions a validation section passes. ✅

#### 5. `requires_files_to_avoid` — **LOW false-positive risk**

Only checks for `avoid`, `blocked`, `do not edit`, `should not`, `must not`, `defer`. Very easy to pass. ✅

#### 6. `no_certain_game_claims` — **LOW false-positive risk**

Specific phrases only. Models would have to use exact language like "this parameter controls exactly" or "definitely causes sky". ✅

#### 7. `no_full_xml_output` — **LOW false-positive risk**

Only fires on actual XML tag patterns with substantial content. ✅

#### 8. `no_unrelated_components` — **MEDIUM false-positive risk**

**Confirmed issue:** The check fires if `(expect|modify|patch|change|edit|update)` appears before a component name like `tracer` or `hit_effect`. The bypass pattern checks for `avoid.{0,60}${component}` or `do not.{0,60}${component}`.

**False-positive case:** A model that writes:

```
Expected changes that SHOULD FAIL validation:
- tracer changes unexpected
- hit_effect changes unexpected
```

The verb `"expected"` is followed by the component name in an obviously safe context. But the regex is `(expect)\\s+tracer` — this **will match** because `expect` precedes `tracer`. The bypass pattern checks for `avoid.{0,60}tracer` — if not present on the same region, **this hard-fails**. **This is a confirmed false-positive: the validation plan section uses "expected" to mean "we expect NOT to see these changes", but the rule fires.**

**Fix:** Extend bypass to include patterns like `should fail`, `unexpected change`, `expect.*not`, `fail validation`, `unexpected.*tracer`, or require the verb+component pattern to NOT be in a validation/fail context.

#### 9. `requires_scanner_grounding` — **MEDIUM false-positive risk**

Checks for: `scanner`, `scan report`, `scanner context`, `scanner output`, `scanner report`, `analysis`, `provided context`.

**The word `analysis` is in the bypass list.** Most model outputs will say "analysis" somewhere. ✅ Low practical false-positive risk.

**Edge case:** A model that writes a very structured JSON-format answer (from the `json-patch-plan` preset) may not use any of these words in the JSON output. If the answer is entirely JSON without prose, none of these keywords appear. **This is a real false-positive risk for JSON-mode outputs.** Recommend: add `"context"` or skip `requires_scanner_grounding` when the answer starts with `{`.

### Warnings — issues

#### `weather_state_files_early` — **MEDIUM false-positive risk (key issue)**

This warning fires when `w_foggy.xml`, `w_clouds.xml`, etc. appear on a line that includes a first-patch keyword. 

**Problem:** The scanner's own `timecycle_file_rankings.json` recommends `w_foggy.xml` and `w_clouds.xml` as `first_patch` candidates. A model correctly following scanner guidance will place these in a first-patch section. The warning then fires even though the model is doing the right thing.

The warning says: "Consider Phase 2 after visualsettings/cloudkeyframes/timecycle_mods_1 validate." This is a legitimate caution, but it should not override scanner guidance that explicitly recommends these files.

**Recommended fix:** Downgrade `weather_state_files_early` from a warning to an info note, or suppress it if the surrounding context makes clear these are second-tier (not the very first files). Alternatively, narrow the trigger: only fire if `w_foggy.xml` appears in a first-patch section that does NOT include a qualifier like "after timecycle/visualsettings" or "phase 2 of first patch".

#### `binary_files_mentioned_safe` — **CONFUSING (not a false positive, but misleading UX)**

This warning fires when binary files are mentioned in a safe/blocked context. It says "Rule check passed." This is technically correct but confusing: it's an informational warning that tells the user everything is fine, yet it appears as a warning. Models that correctly mention binary files as blocked will always trigger this. **Recommend: change to a `passed_with_note` level or suppress entirely.**

### Summary table of rule issues

| Rule | Issue type | Severity | Fix |
|---|---|---|---|
| `no_unrelated_components` | False positive: "expected" used in validation plan context | HIGH | Extend bypass to include `should fail`, `unexpected change`, `fail validation` near component names |
| `weather_state_files_early` (warning) | False positive: fires when scanner recommends w_foggy/w_clouds as first_patch | MEDIUM | Narrow trigger or downgrade to info; suppress when context is "after validating X" |
| `no_weather_xml_first` | `'target files'` in FIRST_PATCH_KWORDS too broad | LOW | Replace with `'target files for first patch'` |
| `requires_scanner_grounding` | JSON-mode answers have no prose keywords | LOW | Skip rule or add `"context"` keyword; or skip when answer starts with `{` |
| `binary_files_mentioned_safe` | Confusing warn for correct behavior | UX | Change to info/note level or suppress |

---

## 9. OpenRouter Error Handling

| Error scenario | Handling | Quality |
|---|---|---|
| HTTP 429 (rate limit) | `server/index.ts` returns `httpStatus: 429, isFreeModel: bool`; `App.tsx` starts 8s cooldown + suggests next model | ✅ good |
| Upstream provider throttling | `orErrorMeta.provider_name` returned in error response | ✅ basic |
| Retry-After header | Not parsed/forwarded | ⚠️ missing — 429 always uses hardcoded 8s cooldown |
| Empty response (`answer === ''`) | Returns `ok: false, code: EMPTY_RESPONSE` | ✅ |
| finish_reason = length | `finishReason` and `nativeFinishReason` returned; `ResultPanel` shows Continue button | ✅ |
| Context too large | OpenRouter returns error message; passed through as `orErr.message` | ✅ basic — no special handling |
| Model not found | OpenRouter returns 404/error; passed through | ✅ basic |
| Credits/spend cap issue | OpenRouter returns error message; passed through | ✅ basic — no special user message |
| Network error (fetch throws) | `catch` returns `code: NETWORK_ERROR` | ✅ |
| Invalid JSON from OpenRouter | `catch JSON.parse` returns `code: PARSE_ERROR` | ✅ |
| Missing API key | Returns `code: MISSING_API_KEY` with setup instruction | ✅ |

**Notable gap:** `Retry-After` header from OpenRouter is not parsed. The cooldown is hardcoded to 8 seconds. For rate-limited free models, OpenRouter sometimes specifies a longer retry window. Low priority but worth adding.

---

## 10. UX Assessment

| UX item | Status |
|---|---|
| Run same test across multiple models | ✅ Compare mode (2 models) + Auto Eval (fixed planner/critic models) |
| Clear which files to upload | ⚠️ `recommendedFiles` list shown in preset UI but uses **old file names** (see §6 above) |
| Selected files visible | ✅ uploaded file list shown with name + size |
| Token counts visible | ✅ usage tokens shown in result panel |
| Cost estimates visible | ✅ cost shown per Auto Eval step + total |
| Auto Eval result readable | ✅ step-by-step status + expandable planner answer + critic scores table |
| False rejections easy to understand | ⚠️ Rule check messages are clear, but `binary_files_mentioned_safe` warning is confusing (appears to be a problem when it's actually OK) |
| Cooldown feedback | ✅ countdown + suggested next model shown |
| Provider switching (Ollama vs OpenRouter) | ✅ clean provider toggle |

---

## 11. Missing Must-Haves Before Next Phase

The next phase is: `patch_plan.json` schema → plan validator → deterministic editor tools.

| Item | Priority | Why needed |
|---|---|---|
| **`patch_plan.json` schema definition** | CRITICAL | Before building validator or editors, the schema must be finalized. Scanner reports now provide correct inputs but there is no agreed output schema. |
| **Plan validator** | HIGH | Auto Eval currently only rule-checks and AI-grades the prose answer. A deterministic validator that checks if a patch plan JSON is safe/well-formed is required before any editor. |
| **Rule checker fixes** (§8 above) | HIGH | `no_unrelated_components` false positive on validation plan context will reject good answers that correctly name tracer/hit_effect as unexpected. |
| **recommendedFiles update** in prompt presets | MEDIUM | Points to old corpus file names; causes confusion about which files to upload. Easy fix. |
| **Rust binary copy step** in build scripts | MEDIUM | Current `build_windows.ps1` and documentation do not mention the manual copy step. Developers building from source will silently get stale binary. |
| **Retry-After header parsing** | LOW | Hardcoded 8s cooldown; OpenRouter sometimes specifies longer. |
| **deepseek free/paid label clarity** | LOW | Both appear in dropdowns with similar names; confusing but not unsafe. |

---

## 12. Recommended Next Implementation Phase

**Phase R0.9.1 — Fixes and patch_plan.json foundation**

1. Fix rule checker `no_unrelated_components` false positive (validation plan context)
2. Fix `weather_state_files_early` warning trigger (too broad for w_foggy/w_clouds)
3. Update `recommendedFiles` in `promptPresets.ts` to match R0.8.1 output file names
4. Document/script the Rust binary copy step in `build_windows.ps1`
5. Define `patch_plan.json` schema v1 (candidateFiles, allowedOperations, blockedFiles, validationRules, requiredTools, goal, schemaVersion)
6. Add scanner command to validate a `patch_plan.json` against baseline scan data

Do not implement editor tools until the validator exists.

---

## 13. Manual Test Checklist

### OpenRouter Model Tester

- [ ] Start backend: `npm run dev` → server on port 3001, API key confirmed
- [ ] Open `http://localhost:5173`
- [ ] Load preset: **Compact Plan** → verify system/user prompts update
- [ ] Upload `ai_timecycle_context_compact.md` from a Redux diff output
- [ ] Run with free model (openrouter/free or owl-alpha) → verify result appears
- [ ] Verify cost = $0.00 for free model
- [ ] Switch to Paid mode → verify confirm modal appears before running
- [ ] Cancel paid confirm → verify no call was made
- [ ] Verify `usedBudgetUsd` stays $0.00 after cancel
- [ ] Force a 429 by running repeatedly → verify cooldown countdown + next model suggestion
- [ ] Check key status → verify balance/credits shown correctly
- [ ] Load **JSON Patch Plan** preset → run → verify JSON output structure

### Auto Eval

- [ ] Switch to Auto Eval tab
- [ ] Upload `ai_timecycle_context_compact.md`
- [ ] Enable rule checks + AI critic
- [ ] Run → verify 6-step pipeline progresses
- [ ] Verify planner answer appears in panel
- [ ] Verify critic score table appears
- [ ] Verify final verdict shown as accepted / usable_with_validator
- [ ] Test a known bad answer (paste one that says "edit weather.xml first") → verify hard fail fires
- [ ] Test a known good answer that mentions tracer in a validation context → **verify this does NOT false-fail** (known bug §8)
- [ ] Save result as Markdown → verify file downloads with correct content
- [ ] Save result as JSON → verify file downloads

### Scanner timecycle reports

- [ ] Run scanner: `.\build\Release\redux_rpf_scanner.exe diff-against-baseline --baseline <baseline.json> --modded <update_modded.rpf> --keys <keys_dir> --out <output_dir> --analyze-text --build-learning-corpus`
- [ ] Verify `timecycle_intelligence/` folder created in output
- [ ] Verify all 9 files present
- [ ] Verify `ai_timecycle_context_compact.md` is under ~2000 words
- [ ] Verify `timecycle_file_rankings.json` has `recommendedPhase` field on all entries
- [ ] Verify `risky_files_report.json` includes weather.xml and timecycle_mods_3.xml
- [ ] Verify `cloudkeyframes_report.json` has `color_only_pattern_confirmed` field
- [ ] Upload `ai_timecycle_context_compact.md` to tester → run Compact Plan → check output quality

### Combined AI testing workflow

- [ ] Use `aggregate_ai_timecycle_context_compact.md` as context in tester
- [ ] Run with free model → verify it references all 3 Reduxes
- [ ] Run with paid model (with confirmation) → compare depth of analysis
- [ ] Run Auto Eval → verify aggregate context produces better scores than single-Redux context
- [ ] Verify no raw game file content appears in model output
- [ ] Verify no keys/RPF paths appear in model output
| `ai_file_explanations.json` | **missing** | No explanation generator; should not be added yet. |
| `ai_training_notes.jsonl` | **missing** | No corpus notes pipeline; should not be added yet. |
| `component_frequency.json` | **missing** | Could later be derived from compare outputs, but not implemented. |
| `redux_making_atlas.md` | **missing** | Not implemented; should be local/generated, not hand-curated from raw assets. |
| `local_ai_context.md` | **missing** | Not implemented. |

### What exists

Almost nothing in this area beyond:

- conceptual roadmap/docs
- JSON outputs that could become upstream inputs later

### What is missing

- extraction
- chunking
- corpus indexing
- AI-facing derivative artifacts
- local-only data management policy in code/scripts

### What should not be implemented yet

Per repo direction, do **not** add AI generation/planning yet. Before any corpus builder:

1. fix scanner build
2. stabilize schema
3. add baseline/full diff outputs
4. add safe analyzers for readable formats

### What should be generated locally only

All derivative outputs from proprietary game data should stay local-only unless explicitly sanitized and legally reviewed:

- extracted readable XML/DAT/META text
- decompiled GFX/SWF artifacts
- texture previews
- analyzer intermediate files
- AI chunk stores
- local context/atlas files derived from real game content

### What should never be committed to Git

- raw `.rpf` archives
- extracted proprietary assets
- keys
- decoded raw game files
- corpus folders built from GTA content
- tool outputs containing private local machine paths if avoidable

## Recommended Roadmap

### R0.1 Scanner foundation cleanup

- **Goal:** restore buildable, runnable scanner foundation.
- **Files likely affected:** `rpf_backend_rs/src/main.rs`, `src/cpp/redux_rpf_scanner.cpp`, `README.md`, `scripts/build_windows.ps1`, `scripts/build_linux.sh`, possibly `CMakeLists.txt`.
- **Features to implement:** fix Rust compile blockers; verify C++→Rust command flow; forward rule flags; clarify packaged backend path expectations; keep existing commands working.
- **Tests to add:** Rust unit tests for compile-fix paths; launcher integration smoke tests; argument-forwarding tests if practical.
- **Acceptance criteria:** `cargo check`, `cargo test`, `cmake --build build`, and basic `version`/`validate-tools` all pass; `scan-rpf` and `compare-rpf` can start successfully with valid inputs.
- **Risks:** easy to accidentally widen scope into schema/analyzer work; avoid that.

### R0.2 Stable JSON schema

- **Goal:** make scan/compare output contract dependable.
- **Files likely affected:** `rpf_backend_rs/src/main.rs`, `README.md`, `examples/sample_outputs/*.json`.
- **Features to implement:** finalize `schemaVersion`, `tool`, `timing`, `scan`, `rules`, archive metadata, structured warnings/errors; update sample outputs.
- **Tests to add:** schema snapshot tests for scan/compare JSON.
- **Acceptance criteria:** generated outputs match docs and samples; no stale sample schema remains.
- **Risks:** breaking downstream assumptions if fields change without version discipline.

### R0.3 Full baseline scan

- **Goal:** produce canonical clean baseline artifacts.
- **Files likely affected:** `rpf_backend_rs/src/main.rs`, new schema/output code, `README.md`, maybe `docs/ARCHITECTURE.md`.
- **Features to implement:** baseline manifest/tree outputs, archive identity metadata, stronger full-scan semantics.
- **Tests to add:** fixture-based scan tests, deterministic output tests.
- **Acceptance criteria:** one clean archive can generate repeatable baseline artifacts with documented names and fields.
- **Risks:** large output size, nested RPF recursion cost, ambiguity between manifest vs tree outputs.

### R0.4 Full modded scan + full diff

- **Goal:** generate full modded manifest and trustworthy diff against baseline.
- **Files likely affected:** `rpf_backend_rs/src/main.rs`, output schema, docs/examples.
- **Features to implement:** compare against baseline artifacts, full added/removed/modified coverage, stronger diff summaries.
- **Tests to add:** compare fixtures covering add/remove/modify and nested cases.
- **Acceptance criteria:** diff is complete, deterministic, and baseline-aware.
- **Risks:** output explosion, nested duplication, false positives from path-only heuristics.

### R0.5 Tree fingerprint + renamed RPF classifier

- **Goal:** identify update-like archives by structure, not filename.
- **Files likely affected:** likely new command handlers in C++ and Rust, new fingerprint structs/output.
- **Features to implement:** tree fingerprint schema, quick tree scan, scoring/classification command.
- **Tests to add:** positive/negative archive classification fixtures.
- **Acceptance criteria:** classifier returns stable label + score + reasons.
- **Risks:** threshold tuning, false positives on similar archives.

### R0.6 Unknown pattern discovery

- **Goal:** preserve and surface novel changes instead of losing them to known-rule bias.
- **Files likely affected:** diff/classification code, new output writers, docs.
- **Features to implement:** unknown text/binary classification, `unknown_changes.json`, candidate-pattern output, review queue.
- **Tests to add:** fixtures with unknown-but-readable and unknown-binary changes.
- **Acceptance criteria:** unknown files are surfaced, not silently filtered, and are triageable.
- **Risks:** noisy outputs, low-signal candidate patterns.

### R0.7 XML/DAT/META analyzers

- **Goal:** start safe deep analysis for readable formats.
- **Files likely affected:** likely new analyzer modules under Rust backend or sibling crate/module, docs/examples.
- **Features to implement:** XML/timecycle first; DAT/META next; structured inside-file diff JSON.
- **Tests to add:** parser/diff fixture tests per format.
- **Acceptance criteria:** deep analyzer outputs identify meaningful internal changes for readable formats.
- **Risks:** format edge cases, parser complexity, overfitting to one Redux sample.

### R0.8 Learning corpus builder

- **Goal:** generate local-only derived artifacts for later AI-assisted tooling.
- **Files likely affected:** new local-only output modules/scripts, `.gitignore`, docs.
- **Features to implement:** extracted text pipeline, chunking, component frequency summaries, local corpus manifests.
- **Tests to add:** local-only pipeline smoke tests, path safety checks.
- **Acceptance criteria:** corpus artifacts are generated locally, excluded from Git, and built only from approved derived content.
- **Risks:** copyright contamination, large local storage use, premature AI coupling.

### R0.9 Linux build + HomeOps integration readiness

- **Goal:** make scanner portable and integration-ready without adding HomeOps logic directly.
- **Files likely affected:** `CMakeLists.txt`, scripts, packaging docs, CI files, maybe command/output polish.
- **Features to implement:** Linux CI, packaging verification, contract docs for job runners, stable exit codes/artifacts.
- **Tests to add:** CI matrix, smoke tests on Windows + Linux.
- **Acceptance criteria:** scanner builds and runs on supported Windows/Linux targets with documented outputs.
- **Risks:** platform path/process differences, packaging drift between helper scripts and real releases.

## Immediate Next Implementation Phase

**Recommend exactly: `R0.1 Scanner foundation cleanup`.**

Why this must be next:

1. Backend does not compile, so every later phase is blocked.
2. Launcher/backend contract still has a real wiring gap for rules flags.
3. Schema work, baseline work, cache work, and analyzer work all depend on having a buildable scanner first.

Scope for that next phase should stay tight:

- fix `E0499` and `E0382` in `rpf_backend_rs/src/main.rs`
- restore passing `cargo check` / `cargo test`
- verify `scan-rpf` and `compare-rpf` runtime path again
- forward rule-file flags from launcher to backend
- do **not** start cache, fingerprint, analyzer, or AI corpus work in same phase

## Files That Need Attention

| File | Why it needs attention |
|---|---|
| `rpf_backend_rs/src/main.rs` | Active compile blockers; central scan/compare/report logic; future schema/baseline work lives here. |
| `src/cpp/redux_rpf_scanner.cpp` | Launcher is mostly solid, but rule-file flags are parsed and never forwarded; backend path assumptions rely on packaged layout. |
| `README.md` | Claims schema-v2 output that does not match committed sample outputs; should later be reconciled with real generated artifacts. |
| `examples/sample_outputs/deep_manifest.json` | Stale sample schema; includes absolute local paths. |
| `examples/sample_outputs/component_diff.json` | Stale sample schema; includes absolute local paths; odd `cleanEntries: 0` baseline example. |
| `scripts/build_windows.ps1` | Useful packager, but should later be aligned with verified full build/test flow. |
| `scripts/build_linux.sh` | Indicates Linux intent, but lacks validation and test coverage. |
| `CMakeLists.txt` | Builds launcher only; no Rust integration, packaging, or tests. |
| `docs/SCANNER_V2_ROADMAP.md` | Good target-state doc, but now ahead of verified repo state. |
| `docs/DEEP_ANALYZER_PLAN.md` | Good planning doc; should remain deferred until foundation/schema/baseline phases are complete. |

## Final Recommendation

Treat this repo as **foundation-stage, not feature-complete**.

Best reading of current state:

- architecture direction = good
- launcher build on Windows = good
- backend logic depth = better than bare skeleton
- end-to-end readiness = blocked
- sample/doc consistency = weak
- roadmap priority = correct, but current repo still needs R0.1 before anything else

If next agent starts implementing from this audit, safest order is:

1. restore compile/run
2. stabilize real output schema and examples
3. add baseline artifacts
4. add cached/full diff flow
5. only then move into unknown pattern discovery and deep analyzers
