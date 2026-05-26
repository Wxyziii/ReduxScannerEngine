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
