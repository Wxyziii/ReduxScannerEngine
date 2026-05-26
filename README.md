# Redux Scanner Engine

Source repository for the AI Redux Maker scanner engine.

This repo contains:

```text
src/cpp/redux_rpf_scanner.cpp   C++ CLI launcher/frontend
rpf_backend_rs/                 Rust backend that opens/scans RPF archives
docs/                           architecture, roadmap, and Copilot tasks
examples/sample_outputs/         sample scan/compare JSON outputs
scripts/                         Windows/Linux build helper scripts
```

## What this project does

The scanner is a **read-only analysis engine** for GTA V `update.rpf` Redux research.

It can:

- scan a clean or modded `update.rpf`
- list target-relevant files
- recursively inspect important nested RPFs
- hash entries
- compare clean vs modded archives
- detect changed Redux components such as:
  - minimap/HUD
  - tracer
  - hit effect / blood spray
  - sky/timecycle
  - kill effect

## What this project must not do

This scanner should not:

- edit `update.rpf`
- write to a GTA V install folder
- extract or provide GTA keys
- copy reference Redux assets
- call AI models directly
- run HomeOps API/server logic
- import `.zip`/`.rar`/`.7z` archives

Other tools/modules handle importing archives, jobs/logs, UI, and AI summaries.

## Requirements

### Windows

- C++17 compiler:
  - MSVC Build Tools, or
  - MinGW-w64
- Rust stable
- Cargo
- CMake optional but recommended

### Linux/Ubuntu

- `build-essential`
- `cmake`
- `rustup` / Rust stable
- `cargo`

## Build Rust backend

```powershell
cd rpf_backend_rs
cargo build --release
```

Linux:

```bash
cd rpf_backend_rs
cargo build --release
```

## Build C++ launcher with CMake

Windows PowerShell:

```powershell
cmake -S . -B build
cmake --build build --config Release
```

Linux:

```bash
cmake -S . -B build
cmake --build build --config Release
```

## Expected distribution layout

The C++ launcher expects the Rust backend here:

Windows:

```text
dist/
├── redux_rpf_scanner.exe
└── tools/
    └── rpf_backend_rs.exe
```

Linux:

```text
dist/
├── redux_rpf_scanner
└── tools/
    └── rpf_backend_rs
```

Use the helper scripts in `scripts/` to create this layout.

## Current commands

```text
redux_rpf_scanner scan-rpf
redux_rpf_scanner compare-rpf
redux_rpf_scanner baseline-scan
redux_rpf_scanner diff-against-baseline
redux_rpf_scanner classify-rpf
redux_rpf_scanner version
redux_rpf_scanner validate-tools --keys <keys_dir>
redux_rpf_scanner scan-rpf --mode <fast|targeted|deep|full>
redux_rpf_scanner compare-rpf --mode <fast|targeted|deep|full>
```

Examples:

```powershell
.\dist\redux_rpf_scanner.exe scan-rpf `
  --archive "path\to\clean_update.rpf" `
  --keys "path\to\rpf_keys" `
  --out "output\scans\deep_manifest.json" `
  --mode deep `
  --depth 4
```

```powershell
.\dist\redux_rpf_scanner.exe compare-rpf `
  --clean "path\to\clean_update.rpf" `
  --modded "path\to\modded_update.rpf" `
  --keys "path\to\rpf_keys" `
  --out "output\diffs\component_diff.json" `
  --mode targeted `
  --depth 2
```

```powershell
.\dist\redux_rpf_scanner.exe validate-tools `
  --keys "path\to\rpf_keys"
```

## Scan modes

`--mode` controls depth and scope:

1. fast: quick preview, minimal hashing, no nested scan.
2. targeted: default Redux-relevant scan (current behavior).
3. deep: targeted scan with nested RPFs up to `--depth`.
4. full: old `--all` behavior (scan all entries).

Defaults and compatibility:

1. No `--mode` provided → `targeted`.
2. `--all` → `full`.
3. `--targets-only` → `targeted`.
4. `--mode` + `--all/--targets-only` → `--mode` wins and a warning is emitted.

Examples:

```powershell
.\dist\redux_rpf_scanner.exe scan-rpf `
  --archive "path\to\clean_update.rpf" `
  --keys "path\to\rpf_keys" `
  --out "output\scans\targeted_manifest.json" `
  --mode targeted `
  --depth 4
```

```powershell
.\dist\redux_rpf_scanner.exe scan-rpf `
  --archive "path\to\clean_update.rpf" `
  --keys "path\to\rpf_keys" `
  --out "output\scans\deep_manifest.json" `
  --mode deep `
  --depth 4
```

```powershell
.\dist\redux_rpf_scanner.exe compare-rpf `
  --clean "path\to\clean_update.rpf" `
  --modded "path\to\modded_update.rpf" `
  --keys "path\to\rpf_keys" `
  --out "output\diffs\component_diff.json" `
  --mode targeted `
  --depth 4
```

## Baseline scan (R0.3)

`baseline-scan` scans a **clean** `update.rpf` once and writes 4 artifacts to a folder.

```powershell
.\dist\redux_rpf_scanner.exe baseline-scan `
  --archive "path\to\clean_update.rpf" `
  --keys "path\to\rpf_keys" `
  --out "output\baseline" `
  --depth 4
```

Output folder contains:

```text
output\baseline\
├── full_clean_manifest.json          all entries with metadata + isTextCandidate flags
├── full_clean_tree.json              structure summary: top folders, extension counts, path list
├── baseline_update_tree_fingerprint.json  deterministic fingerprint + anchor path check
└── baseline_metadata.json           identity + scanner/schema/rules version for cache validation
```

### When to rebuild the baseline

Rebuild the baseline when:

- The clean `update.rpf` changes (new GTA patch)
- Scanner or schema version changes
- Rules version changes
- You explicitly want a fresh baseline

The `baseline_metadata.json` records the archive sha256, scanner version, and schema version. Check these before reusing a cached baseline.

### Baseline artifacts must not be committed

Do not commit baseline output artifacts produced from real game files. They are derived from proprietary game data. Store them locally only.

### full_clean_manifest.json

Per-file entry shape:

```json
{
  "path": "common/data/timecycle/timecycle_mods_1.xml",
  "name": "timecycle_mods_1.xml",
  "extension": "xml",
  "sizeBytes": 47001,
  "sha256": "...",
  "source": "path/to/update.rpf",
  "isTextCandidate": true,
  "isBinaryCandidate": false
}
```

### baseline_update_tree_fingerprint.json

Shape summary:

```json
{
  "schemaVersion": "2.0",
  "artifactType": "baseline_update_tree_fingerprint",
  "totalPaths": 12345,
  "treeFingerprintSha256": "...",
  "topLevelFolders": ["common", "x64", "dlc_patch"],
  "extensionHistogram": [{"extension": "yvr", "count": 3692}, {"extension": "gxt2", "count": 8041}, ...],
  "anchorPathsFound": ["american_rel.rpf/", "ptfx.rpf/", "visualsettings.dat", "gta5_cache_y.dat"],
  "anchorPathsMissing": ["scaleform_frontend.rpf/", "popcycle.dat"]
}
```

`treeFingerprintSha256` is a SHA-256 of sorted `"path:size"` strings. Deterministic for identical archives.

## classify-rpf (R0.5 / R0.5.1)

`classify-rpf` quick-scans an unknown or renamed `.rpf` file and compares its internal tree structure against the clean baseline fingerprint to detect whether it is a `update.rpf` variant.

**Why this exists:** Redux mod packages sometimes distribute their modded `update.rpf` under a different name (e.g., `redux.rpf`, `modded.rpf`, `main.rpf`). The classifier detects these renamed files so they can be correctly imported as `update.rpf` replacements.

```powershell
.\dist\redux_rpf_scanner.exe classify-rpf `
  --archive "path\to\unknown_archive.rpf" `
  --baseline "path\to\baseline_output_folder" `
  --keys "path\to\rpf_keys" `
  --out "path\to\classification.json" `
  --depth 3
```

The `--baseline` folder must contain `baseline_update_tree_fingerprint.json` and `baseline_metadata.json` (produced by `baseline-scan`).

### Classification labels

| Label | Score | Recommended action |
|---|---|---|
| `obvious_update_rpf` | 90–100 | `import_as_update_rpf` |
| `likely_update_rpf` | 75–89 | `import_as_update_rpf` |
| `possible_update_rpf` | 50–74 | `review_before_import` |
| `not_update_rpf` | 20–49 | `skip` |
| `unknown_rpf` | 0–19 | `review` |
| `scan_failed` | n/a | `review_error` |

### Scoring

The classifier scores the archive based on:

- **Anchor file matches** — presence of characteristic `update.rpf` files (`visualsettings.dat`, `gta5_cache_y.dat`, `popcycle.dat`, `carcols.meta`, `hudcolor.dat`) and nested RPF prefixes (`american_rel.rpf/`, `ptfx.rpf/`, `scaleform_frontend.rpf/`)
- **Extension signals** — `.yvr` (route data), `.ysc` (scripts), `.gxt2` (text strings), `.ymap` (world data), `.fxc` (shaders) are strongly characteristic of `update.rpf`
- **Entry count** — large archives (>5000 entries) score higher; very small archives are penalized
- **Size ratio** — entry count compared to the clean baseline; archives that are 30–150% of baseline size score a bonus
- **Narrow-archive penalties** — vehicle-only or audio-only archives (no script/text files) are penalized

### GTA V NG encryption and logical-name fallback (R0.5.1)

GTA V uses NG encryption where the decryption key is derived from the archive's **filename**. If a `update.rpf` is byte-copied and renamed to `redux.rpf`, opening it under the new name causes wrong key derivation — nested archives cannot be decrypted and entries appear as garbled bytes.

**R0.5.1 adds a logical-name fallback** to handle this case automatically:

1. The classifier first scans the archive under its physical filename.
2. If the initial scan score is below 50 (archive appears unreadable or unrelated), and the physical name is not already `update.rpf`, the classifier transparently:
   - Creates a temporary directory
   - Copies the archive to `<temp>/update.rpf` (read-only operation; source is not modified)
   - Scans the temp copy — now GTA NG key derivation uses `update.rpf` as the key name
   - Cleans up the temp copy automatically
3. If the fallback scan scores higher than the physical scan, the fallback result is used.
4. The output reports both attempts in the `attempts` array so the caller can see what happened.

**Important:** The fallback uses the same scoring thresholds. An unrelated archive that happens to open under the `update.rpf` name will still score low if its tree doesn't match the baseline. The fallback does not lower the quality bar.

### Output shape (R0.5.1)

```json
{
  "schemaVersion": "2.0",
  "ok": true,
  "artifactType": "rpf_classification",
  "classification": "obvious_update_rpf",
  "confidence": 1.0,
  "score": 100,
  "recommendedAction": "import_as_update_rpf",
  "reasons": ["Archive matched update.rpf tree when opened with logical name \"update.rpf\"...", "..."],
  "matchedAnchors": ["american_rel.rpf/", "visualsettings.dat"],
  "missingAnchors": [],
  "extensionHistogram": [{"extension": "yvr", "count": 3200}, ...],
  "archive": { "path": "...", "fileName": "redux.rpf", "sizeBytes": 0, "sha256": "...", "entryCount": 14449 },
  "baseline": { "archiveFileName": "update.rpf", "treeFingerprintSha256": "..." },
  "attempts": [
    {
      "physicalFileName": "redux.rpf",
      "logicalFileName": "redux.rpf",
      "entryCount": 541,
      "score": 0,
      "classification": "unknown_rpf",
      "usedForResult": false,
      "note": null
    },
    {
      "physicalFileName": "redux.rpf",
      "logicalFileName": "update.rpf",
      "entryCount": 14449,
      "score": 100,
      "classification": "obvious_update_rpf",
      "usedForResult": true,
      "note": "Archive matched update.rpf tree when opened with logical name \"update.rpf\"."
    }
  ],
  "usedLogicalArchiveName": "update.rpf",
  "warnings": []
}
```

- `attempts` — array of scan attempts (physical name first, then logical fallback if triggered)
- `usedLogicalArchiveName` — `null` if the physical-name scan was sufficient; `"update.rpf"` if the fallback was used

See `examples/sample_outputs/classify_rpf_example/classification.json` for a full sanitized example.

### Do not commit real classification artifacts

Do not commit `classification.json` outputs generated from real game files. They are derived from proprietary game data. Store them locally only.

All scan and compare JSON reports use `schemaVersion: "2.0"`.

### Scan output shape

```json
{
  "schemaVersion": "2.0",
  "ok": true,
  "tool": {
    "name": "rpf_backend_rs",
    "version": "0.2.0",
    "backendVersion": "0.2.0",
    "platform": "windows"
  },
  "timing": {
    "startedAt": "2025-01-01T00:00:00Z",
    "finishedAt": "2025-01-01T00:00:01Z",
    "durationMs": 1234
  },
  "scan": {
    "mode": "targeted",
    "depth": 3,
    "archivePath": "path/to/update.rpf",
    "archiveFileName": "update.rpf",
    "archiveSizeBytes": 1048576,
    "archiveSha256": "...",
    "keysPathProvided": true
  },
  "rules": {
    "componentRulesSource": "fallback",
    "componentRulesPath": null,
    "componentRulesVersion": "built-in",
    "targetRulesSource": "fallback",
    "targetRulesPath": null,
    "targetRulesVersion": "built-in",
    "rulesDir": null,
    "usedFallbackRules": false
  },
  "stats": {
    "totalEntries": 284,
    "scannedEntries": 284,
    "targetEntries": 284,
    "nestedArchivesOpened": 2,
    "warnings": 0
  },
  "warnings": [],
  "files": [
    {
      "path": "common/data/bloodfx.dat",
      "name": "bloodfx.dat",
      "extension": "dat",
      "sizeBytes": 47001,
      "sha256": "...",
      "source": "path/to/update.rpf"
    }
  ]
}
```

### Compare output shape

```json
{
  "schemaVersion": "2.0",
  "ok": true,
  "tool": { "..." },
  "timing": { "..." },
  "scan": {
    "mode": "targeted",
    "depth": 3,
    "clean": {
      "archivePath": "path/to/clean_update.rpf",
      "archiveFileName": "clean_update.rpf",
      "archiveSizeBytes": 1048576,
      "archiveSha256": "..."
    },
    "modded": {
      "archivePath": "path/to/modded_update.rpf",
      "archiveFileName": "modded_update.rpf",
      "archiveSizeBytes": 1052672,
      "archiveSha256": "..."
    },
    "keysPathProvided": true
  },
  "rules": { "..." },
  "stats": {
    "cleanEntries": 284,
    "moddedEntries": 284,
    "addedEntries": 0,
    "removedEntries": 0,
    "modifiedEntries": 12,
    "unchangedEntries": 272,
    "componentReports": 3,
    "warnings": 0
  },
  "warnings": [],
  "components": [ "..." ],
  "allChanges": [ "..." ]
}
```

### Per-file change entry (allChanges)

```json
{
  "path": "x64/patch/data/effects/ptfx.rpf/core.ypt",
  "status": "modified",
  "cleanSize": 3324239,
  "moddedSize": 3330000,
  "cleanSha256": "...",
  "moddedSha256": "...",
  "extension": "ypt",
  "basename": "core.ypt",
  "parentPath": "x64/patch/data/effects/ptfx.rpf",
  "sizeDelta": 5761,
  "sizeDeltaPercent": 0.17,
  "category": "particle",
  "components": ["tracer"],
  "editorNeeded": ["OpenIV"],
  "risk": "medium",
  "likelyPattern": "ptfx_particle_container",
  "confidence": "medium",
  "warning": null,
  "reason": "size and sha256 differ"
}
```

`sizeDeltaPercent` is `null` for added files (no clean baseline).

### Warnings shape

```json
{
  "code": "NESTED_RPF_OPEN_FAILED",
  "severity": "warning",
  "path": "x64/patch/data/effects/ptfx.rpf",
  "message": "failed to open nested RPF: ..."
}
```

### Sample outputs

Files in `examples/sample_outputs/` are **sanitized schema examples** — they use placeholder archive paths and do not contain real game data. No RPF files, keys, or raw game assets are committed to this repo.

## Recommended next development step

Read:

```text
docs/SCANNER_V2_ROADMAP.md
docs/COPILOT_TASKS.md
.github/copilot-instructions.md
```

Start with **Task 1: version + validate-tools**. Do not rewrite the whole scanner at once.

---

## Unknown pattern discovery (R0.6)

After `diff-against-baseline`, the scanner automatically generates unknown-pattern discovery artifacts alongside the standard diff files. These capture everything the component classifier did **not** recognize, so new Redux patterns can be discovered by future analyzers or LLM review.

### Why it exists

The component classifier only knows about explicitly defined components (tracer, hit_effect, sky_timecycle, etc.). A real Redux mod changes hundreds of files that don't match any known rule. R0.6 captures all those unknown changes so they are not silently ignored.

### Generated artifacts

All files are written to the same `--out` folder as the rest of the diff:

| File | Description |
|------|-------------|
| `unknown_changes.json` | Full list of changes where no component matched |
| `unknown_text_candidates.json` | Subset: readable/config files (.xml, .dat, .meta, .ymt, etc.) |
| `unknown_binary_candidates.json` | Subset: binary files requiring a dedicated analyzer |
| `candidate_patterns.json` | Changes grouped by extension and nested archive prefix |
| `llm_review_queue.jsonl` | Metadata-only tasks for future LLM review (one JSON object per line) |
| `unknown_summary.json` | Compact summary counts + top extensions + top folders |

### Unknown classes

Each entry in `unknown_changes.json` has an `unknownClass` field:

| Class | Meaning |
|-------|---------|
| `unknown_config_candidate` | Extension like `.xml`, `.dat`, `.meta`, `.ini` — likely readable |
| `unknown_text_candidate` | Extension like `.ymt`, `.ymap`, `.ytyp` — possibly readable |
| `unknown_binary_candidate` | Extension like `.ytd`, `.ypt`, `.gfx`, `.ydr` — needs analyzer |
| `unknown_nested_archive_candidate` | Nested `.rpf` archive — needs recursive scan |
| `unknown_low_priority` | Small file, unknown extension, low impact |

### llm_review_queue.jsonl

This file contains **metadata only** — no file contents, no raw assets. Each line is a JSON object:

```json
{"task":"review_unknown_change","path":"ambientpedmodelsets.meta","status":"modified","extension":"meta","unknownClass":"unknown_config_candidate","context":{"folder":"root","nestedArchivePath":null,"sizeDeltaBytes":979},"question":"What GTA/Redux component might this changed file relate to? Answer as hypothesis only."}
```

**The scanner does not call any LLM.** This file is a queue for an external pipeline or human review.

### candidate_patterns.json

Changes are grouped by shared extension and nested archive prefix. Each group with 2+ files becomes a candidate pattern:

```json
{
  "patternId": "pattern_001",
  "title": "Unknown .dat cluster",
  "candidateComponent": "config_or_text",
  "confidence": "high",
  "evidence": ["shared extension: dat", "file count: 28"],
  "files": ["decals.dat", "entityfx.dat", "..."],
  "recommendedNextStep": "run DAT/META/XML analyzer in R0.7"
}
```

### Do not commit real unknown-pattern artifacts

Do not commit `unknown_changes.json`, `candidate_patterns.json`, `llm_review_queue.jsonl`, or any other diff output generated from real game files. They are derived from proprietary GTA V data. Store them locally only.

See `examples/sample_outputs/unknown_patterns_example/` for sanitized examples with fake paths and hashes.



## R0.7: Text / Config Inside-File Analyzers

After `diff-against-baseline` identifies changed files, add `--analyze-text` to compare
the actual byte contents of readable text, XML, DAT, and META files between the clean
and modded archives.

### Usage

```
redux_rpf_scanner.exe diff-against-baseline ^
  --modded <modded_update.rpf> ^
  --baseline <baseline_output_dir> ^
  --keys <keys_dir> ^
  --out <diff_output_dir> ^
  --clean <clean_update.rpf> ^
  --analyze-text
```

`--analyze-text` is **optional**. Without it, only the standard R0.4/R0.6 diff artifacts
are produced. Adding it triggers inside-file analysis and writes 7 additional artifacts.

`--clean` must be provided (or stored in `baseline_metadata.json` from a fresh baseline-scan)
so the scanner can re-open the clean archive to extract original file bytes.

### Artifacts produced

| File | Description |
|------|-------------|
| `text_analysis_summary.json` | Counts of analyzed / skipped / failed files, top changed files, top extensions |
| `xml_diffs.json` | Line-based diff for `.xml` files with numeric and color-like change detection |
| `dat_diffs.json` | Line and key-value diff for `.dat` config files with numeric deltas |
| `meta_diffs.json` | Diff for `.meta` files (treated as XML-like text) |
| `generic_text_diffs.json` | Line diff for `.txt`, `.ini`, `.cfg`, `.json`, and other readable text |
| `analyzer_warnings.json` | Files skipped due to binary content, extraction failures, or UTF-8 errors |
| `ai_readable_change_notes.jsonl` | Metadata-only JSONL — one record per analyzed file for future LLM review |

### What the analyzers do

- **XML analyzer** — splits lines, detects numeric and color-like value changes, reports
  sample line pairs. Does not use a full DOM parser; uses line-level heuristics.
- **DAT analyzer** — detects `key=value`, `key: value`, and `key value` patterns; diffs
  key sets; reports numeric deltas for changed values.
- **META analyzer** — same approach as XML analyzer (`.meta` files follow XML-like structure).
- **Generic text analyzer** — line-based diff for all other readable text formats.
- **Binary skipped** — `.ymt`, `.ymap`, `.ytyp` files that fail UTF-8 detection are
  logged in `analyzer_warnings.json` as `skippedNotTextBytes`. They remain in
  `unknown_binary_candidates.json` for future binary analyzers.

### No LLM calls

`ai_readable_change_notes.jsonl` contains **metadata and change summaries only**.
No raw file contents are included. No LLM API is called. The records are queued
for optional human or LLM review at a later stage.

### Do not commit real analyzer output

Do not commit `xml_diffs.json`, `dat_diffs.json`, `meta_diffs.json`, or any other
text analyzer output generated from real GTA V game files. They are derived from
proprietary content. Store them locally only.

See `examples/sample_outputs/text_analyzers_example/` for sanitized examples with
fake paths and fake values.


## R0.8: Learning Corpus Builder

After `diff-against-baseline`, add `--build-learning-corpus` to generate a structured
local AI-readable corpus from all scan, diff, unknown-pattern, and text-analyzer outputs.

### Usage

```
redux_rpf_scanner.exe diff-against-baseline ^
  --modded <modded_update.rpf> ^
  --baseline <baseline_output_dir> ^
  --keys <keys_dir> ^
  --out <diff_output_dir> ^
  --clean <clean_update.rpf> ^
  --analyze-text ^
  --build-learning-corpus
```

`--build-learning-corpus` is **optional**. It can be combined with `--analyze-text` for
maximum corpus coverage, or used alone for a file-level-only corpus.

The corpus is written to `<diff_output_dir>/learning_corpus/`.

### Artifacts produced

| File | Description |
|------|-------------|
| `learning_corpus_index.json` | Index of all corpus artifacts with totals and source list |
| `component_frequency.json` | Known/unknown component change counts, top extensions, top paths |
| `file_type_frequency.json` | Per-extension change stats with analyzer status |
| `analyzer_coverage.json` | R0.7 coverage summary with coveragePercent |
| `corpus_ai_change_notes.jsonl` | One AI-readable note per analyzed text file with hypothesis |
| `component_lessons.jsonl` | One lesson per component with evidence and recommended next step |
| `file_lessons.jsonl` | One lesson per important changed file with impact metrics |
| `training_candidates.jsonl` | Candidate supervised examples for future AI training (unreviewed) |
| `local_ai_context.md` | Human/AI-readable Markdown context: what changed, what is safe |
| `redux_making_atlas.md` | High-level component map, pattern summary, and tool recommendations |

### What the corpus builder does

- Aggregates all diff, unknown-pattern, and text-analyzer outputs into one structured corpus
- Groups changes by component and extension for frequency analysis
- Generates cautious hypotheses (never overclaims game meaning)
- Marks all training candidates as `candidate_unreviewed`
- Produces two Markdown reports (`local_ai_context.md`, `redux_making_atlas.md`) that
  summarize what is safe to reason about and what requires future work

### No LLM calls

The corpus builder is entirely local and deterministic. No LLM or AI API is called.
`corpus_ai_change_notes.jsonl` and `training_candidates.jsonl` are **metadata-only** queues
for future human review or optional LLM processing — they contain no raw file contents.

All hypotheses in the corpus are explicitly marked as unconfirmed.

### For future AI / RAG / training use

The `learning_corpus/` folder is designed to be:
- read by a future RAG system to answer questions about Redux components
- used as a candidate dataset for supervised learning (after human review)
- used to guide AI-assisted Redux design planning in the HomeOps project

`training_candidates.jsonl` must be reviewed and relabeled before any actual training use.

### Do not commit real corpus artifacts

Do not commit `learning_corpus/` folders generated from real GTA V game files. They are
derived from proprietary content. Store them locally only.

See `examples/sample_outputs/learning_corpus_example/` for sanitized examples with
fake paths and hashes.
