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
redux_rpf_scanner version
redux_rpf_scanner validate-tools --keys <keys_dir>
redux_rpf_scanner scan-rpf --mode <fast|targeted|deep|full>
redux_rpf_scanner compare-rpf --mode <fast|targeted|deep|full>
```

Examples:

```powershell
.\dist\redux_rpf_scanner.exe scan-rpf `
  --archive "D:\AI_Redux_Lab\baselines\update.rpf" `
  --keys "C:\Users\Marcel\Downloads\rpf_keys" `
  --out "D:\AI_Redux_Lab\scans\clean_deep_scan\deep_manifest.json" `
  --mode deep `
  --depth 4
```

```powershell
.\dist\redux_rpf_scanner.exe compare-rpf `
  --clean "D:\AI_Redux_Lab\baselines\update.rpf" `
  --modded "D:\AI_Redux_Lab\redux_sources\redux_001\update.rpf" `
  --keys "C:\Users\Marcel\Downloads\rpf_keys" `
  --out "D:\AI_Redux_Lab\diffs\clean_vs_redux_001\component_diff.json" `
  --mode targeted `
  --depth 2
```

```powershell
.\dist\redux_rpf_scanner.exe validate-tools `
  --keys "C:\Users\Marcel\Downloads\rpf_keys"
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
  --archive "D:\AI_Redux_Lab\baselines\update.rpf" `
  --keys "C:\Users\Marcel\Downloads\rpf_keys" `
  --out "D:\AI_Redux_Lab\scans\targeted_manifest.json" `
  --mode targeted `
  --depth 4
```

```powershell
.\dist\redux_rpf_scanner.exe scan-rpf `
  --archive "D:\AI_Redux_Lab\baselines\update.rpf" `
  --keys "C:\Users\Marcel\Downloads\rpf_keys" `
  --out "D:\AI_Redux_Lab\scans\deep_manifest.json" `
  --mode deep `
  --depth 4
```

```powershell
.\dist\redux_rpf_scanner.exe compare-rpf `
  --clean "D:\AI_Redux_Lab\baselines\update.rpf" `
  --modded "D:\AI_Redux_Lab\redux_sources\redux_001\update.rpf" `
  --keys "C:\Users\Marcel\Downloads\rpf_keys" `
  --out "D:\AI_Redux_Lab\diffs\clean_vs_redux_001\component_diff.json" `
  --mode targeted `
  --depth 4
```

## Output metadata (schema v2)

Scan and compare JSON reports include schema metadata, tool metadata, timing, and structured warnings:

```json
{
  "schemaVersion": "2.0",
  "tool": {
    "name": "redux_rpf_scanner",
    "version": "0.2.0",
    "backend": "rpf_backend_rs",
    "backendVersion": "0.2.0",
    "platform": "windows"
  },
  "timing": {
    "startedAt": "2026-05-25T19:42:01Z",
    "finishedAt": "2026-05-25T19:42:04Z",
    "durationMs": 2987
  },
  "warnings": [
    {
      "code": "NESTED_RPF_OPEN_FAILED",
      "severity": "warning",
      "path": "x64/patch/data/effects/ptfx.rpf",
      "message": "failed to open nested RPF: ..."
    }
  ]
}
```

Scan and compare outputs also include scan settings:

```json
{
  "scan": {
    "mode": "targeted",
    "depth": 4
  }
}
```

### Compare output rich fields (per changed file)

Each entry in `allChanges` and each component file hit includes rich metadata:

```json
{
  "path": "x64/patch/data/effects/ptfx.rpf",
  "status": "modified",
  "cleanSize": 120001,
  "moddedSize": 118420,
  "sizeDelta": -1581,
  "sizeDeltaPercent": -1.316,
  "extension": "rpf",
  "basename": "ptfx.rpf",
  "parentPath": "x64/patch/data/effects",
  "category": "particle_container",
  "components": ["tracer", "hit_effect"],
  "editorNeeded": ["ypt_particle_editor"],
  "risk": "medium_high",
  "likelyPattern": "particle_container_reduction",
  "confidence": "medium",
  "warning": "Exact particle-level changes require a YPT analyzer."
}
```

`sizeDeltaPercent` is `null` for added files (no clean baseline).

## Recommended next development step

Read:

```text
docs/SCANNER_V2_ROADMAP.md
docs/COPILOT_TASKS.md
.github/copilot-instructions.md
```

Start with **Task 1: version + validate-tools**. Do not rewrite the whole scanner at once.
