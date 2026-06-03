# Scanner v2 Roadmap

## Completed Milestones

- **Phase 1: Build and CLI Foundation.** Versioning, tool validation, and mode support.
- **Phase T0.1: Patch Plan Schema V1.** JSON structure for AI planning.
- **Phase T0.2: Deterministic Safety Validator.** Verification of plan safety.
- **Phase T0.3: XML/DAT Static Validators.** Rust-native syntax and scope checks.
- **Phase T0.4.1: Editor Contract + Dry-Run.** Safety framework for mutations.

## Active Phase: T0 — Controlled Patch Prototype

Goal: Build the end-to-end safety and mutation framework for the first controlled patch.

- [x] T0.1: Patch Plan Schema V1
- [x] T0.2: Deterministic Safety Validator
- [x] T0.3: XML/DAT Static Validators
- [x] T0.4.1: Editor Contract + Dry-Run Framework
- [ ] T0.4.2: XML/DAT Mutation Logic (Upcoming)
- [ ] T0.4.3: Backup & Rollback Logic
- [ ] T0.5: First Controlled Patch Application

## Phase 1 — Build and CLI foundation (Legacy Roadmap)

Goal: make the scanner easier to use, validate, and package.

Tasks:

- Add `version` command to C++ launcher.
- Add `version` command to Rust backend.
- Add `validate-tools` command.
- Validate backend exists.
- Validate keys folder exists.
- Validate these key files:
  - `gtav_aes_key.dat`
  - `gtav_ng_key.dat`
  - `gtav_ng_decrypt_tables.dat`
- Create output parent folder automatically before scan/compare.
- Keep existing `scan-rpf` and `compare-rpf` behavior.

## Phase 2 — Stable JSON schema v2

Goal: make output reliable for HomeOps and local AI.

Add to scan and compare outputs:

```text
schemaVersion
tool metadata
backend metadata
platform
scan mode
target rules version
archive metadata
timings/durationMs
structured warnings
```

Structured warnings should be objects with:

```text
code
severity
path
message
```

## Phase 3 — Rich file/component metadata

Add per-file metadata:

```text
extension
category
components
editorNeeded
risk
reason
```

Suggested categories:

```text
timecycle_xml
weather_xml
particle_container
scaleform_ui
texture_dictionary
minimap_texture
blood_effect_config
weapon_effect_config
optimization_candidate
unknown_binary
```

Suggested components:

```text
minimap_hud
tracer
hit_effect
kill_effect
sky_timecycle
timecycle_weather
texture_optimization
clutter_removal
unknown
```

## Phase 4 — Rich compare output

Add to changed entries:

```text
sizeDelta
sizeDeltaPercent
changeKind
likelyPattern
confidence
warning
```

`sizeDeltaPercent` should be `null` for added files (no clean baseline).

Likely patterns:

```text
particle_container_reduction
timecycle_weather_restyle
minimap_hud_restyle
texture_dictionary_change
asset_added
asset_removed
unknown_binary_change
```

## Phase 5 — Scan modes

Replace only `--all` vs `--targets-only` with:

```text
--mode fast
--mode targeted
--mode deep
--mode full
```

Compatibility mapping:

```text
default (no --mode): targeted
--all: full
--targets-only: targeted
--mode + --all/--targets-only: --mode wins (warn)
```

Suggested meanings:

```text
fast:
  path/size quick scan, minimal hashing

targeted:
  Redux-relevant files only

deep:
  target files + nested target RPFs + full hashes

full:
  everything possible, slow
```

## Phase 6 — Rules file

Move component/target rules out of hardcoded Rust logic into JSON:

```text
rules/component_rules.json
rules/target_rules.json
```

Keep built-in fallback rules so the scanner still works without the files.

## Phase 7 — Cache

Add cache for repeated scans.

Cache key should include:

```text
archive SHA256
archive size
scanner version
backend version
scan mode
depth
target rules version
```

Cache should live somewhere like:

```text
.cache/scanner/
```

## Phase 8 — First deep analyzer: XML/timecycle

Create a separate XML/timecycle analyzer.

It should compare clean vs modded XML files and report changed nodes/values.

This is the first generation-related step because XML/timecycle is safer than YPT/GFX/YTD.
