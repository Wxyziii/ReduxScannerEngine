# Copilot Instructions — Redux Scanner Engine

You are working on the AI Redux Maker scanner engine.

## Architecture

- `src/cpp/redux_rpf_scanner.cpp` is the C++ CLI launcher/frontend.
- `rpf_backend_rs/src/main.rs` is the Rust backend that actually opens RPF archives, scans entries, hashes files, compares clean vs modded, and writes JSON.
- The scanner is read-only.
- The scanner should remain independent from HomeOps UI, Tauri, local AI, and archive-importing logic.

## Hard rules

Do not add:

- UI logic
- Tauri logic
- HomeOps server APIs
- AI calls
- archive `.zip/.rar/.7z` importing
- file editing
- RPF writing
- GTA install folder writes
- key extraction
- raw reference asset copying

Do not commit:

- GTA keys
- `.rpf` files
- extracted proprietary assets
- downloaded Redux archives

## Main goal

Evolve this into Scanner v2:

1. Linux-ready builds.
2. `version` command.
3. `validate-tools` command.
4. Stable JSON schema v2.
5. Structured warnings.
6. Rich compare output:
   - sizeDelta
   - sizeDeltaPercent
   - category
   - components
   - editorNeeded
   - risk
   - likelyPattern
7. Scan modes:
   - fast
   - targeted
   - deep
   - full
8. Component rules file.
9. Scanner cache.
10. Later XML/timecycle deep analyzer.

## Development style

- Make small focused changes.
- Preserve existing `scan-rpf` and `compare-rpf` behavior.
- Add tests where practical.
- Keep all output JSON backward-compatible where possible.
- Prefer clear errors and structured warnings over crashes.
- Keep CLI help up to date.

## First recommended task

Implement:

```text
version
validate-tools
output parent folder creation
```

Do not implement scan modes/cache/rules in the first task.
