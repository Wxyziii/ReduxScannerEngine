# T0.4.1 — Deterministic Editor Contract + Dry-Run Framework

## Purpose
This phase establishes the shared safety framework and contract for future deterministic editors. It provides a read-only dry-run execution path to verify that proposed operations are safe and comply with architectural constraints before any files are modified.

## Shared Contract
The framework defines standard types for representing editor operations, plans, and results.

### Editor Operation (`EditorOperation`)
Standardized fields for every mutation attempt:
- **id:** Unique operation identifier.
- **phase:** Must be `first_patch` for the current prototype.
- **path:** The relative path to the game file.
- **tool:** Approved editor tool name (e.g., `dat_named_key_editor`).
- **type:** Approved operation type (e.g., `xml_color_like_candidate`).
- **valueTarget:** The proposed value change.
- **intent:** Must include hypothesis-style wording (e.g., "Hypothesis", "Believe", "Likely").
- **validationRequired:** Array of validators that must pass.

## Dry-Run Framework
The `editor-dry-run` command simulates the execution of a patch plan.

### Safety Checks
The framework rejects operations that violate these hard rules:
- **Apply Mode:** Real file modification is strictly blocked in this phase.
- **Phase:** Only `first_patch` operations are allowed.
- **Scope:** Rejects blocked/deferred files (e.g., `weather.xml`), binary files, RPF archives, and unrelated components.
- **Tools:** Rejects unknown tools.
- **Intent:** Enforces hypothesis-based reasoning.
- **Validation:** Requires at least one validator to be specified.

### Validation Hook Planning
Dry-run identifies which validators *would* run for each operation:
- **XML:** Parse check, color-like-only, no-node-deletion.
- **DAT:** Parse check, named-key validation.
- **Global:** Scanner scope validation.

## CLI Usage

### Dry-run all operations in a plan
```bash
rpf_backend_rs editor-dry-run --patch-plan <path_to_plan.json>
```

### Dry-run a single operation
```bash
rpf_backend_rs editor-dry-run --patch-plan <path_to_plan.json> --operation-id op_001
```

### JSON Output
Use `--out <file.json>` to capture the structured dry-run report.

## Limitations
- **Read-only:** This phase does not edit files, create backups, or modify RPF archives.
- **Mock logic:** It proves safety and intent compliance but does not calculate the actual byte-level diffs yet.

## Next Steps (T0.4.2)
Future phases will implement the actual mutation logic for XML and DAT files using the safety boundaries established by this framework.
