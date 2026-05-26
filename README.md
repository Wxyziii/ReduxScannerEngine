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

## classify-rpf (R0.5)

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

### GTA V NG encryption note

GTA V uses NG encryption where the decryption key is derived from the archive's **filename**. If a `update.rpf` is copied and renamed to `redux.rpf`, the scanner cannot decrypt its nested archives under the new name — the nested entries appear as garbled bytes.

This means:
- A file that was originally `update.rpf` and renamed as a byte-identical copy → `unknown_rpf` (scanner cannot read nested content)
- A file that was legitimately named `redux.rpf` when built → will scan correctly if the RPF library can decrypt it

If a Redux release distributes a renamed `update.rpf` replacement as `redux.rpf`, classifying it requires first renaming the file back to `update.rpf` for key derivation, or the mod author must distribute it without NG encryption (open RPF format).

### Output shape

```json
{
  "schemaVersion": "2.0",
  "ok": true,
  "artifactType": "rpf_classification",
  "classification": "likely_update_rpf",
  "confidence": 0.86,
  "score": 86,
  "recommendedAction": "import_as_update_rpf",
  "reasons": ["..."],
  "matchedAnchors": ["american_rel.rpf/", "visualsettings.dat"],
  "missingAnchors": ["popcycle.dat"],
  "extensionHistogram": [{"extension": "yvr", "count": 3200}, ...],
  "archive": { "path": "...", "fileName": "...", "sizeBytes": 0, "sha256": "..." },
  "baseline": { "archiveFileName": "update.rpf", "treeFingerprintSha256": "..." },
  "warnings": []
}
```

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
