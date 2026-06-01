# T0.3 — XML/DAT Static Validators

## Purpose
This phase builds read-only static validators that verify XML/DAT files and patch-plan scope before any editor tools are built or patches applied.

## Validators

### XML Validator (`xml_validator`)
Validates XML files for syntax and structure.
- **Parse check:** Ensures the file is valid XML.
- **Structure preservation:** Compares against a baseline to ensure no nodes were deleted and the tag hierarchy remains intact.
- **Color-like-only check:** Ensures only color-related values were changed. Numeric-only changes to non-color fields or structural changes are flagged as errors.

### DAT Validator (`dat_validator`)
Validates `visualsettings.dat` and similar named-key DAT files.
- **Parse check:** Handles normal text DAT lines, comments, and blank lines.
- **Named-key detection:** Detects `key value` or `key=value` pairs.
- **Family validation:** Only allows changes to `Adaptation` and `Tonemapping` families for the first prototype. Flags `adaptivedof` with warnings.
- **Diff against baseline:** Identifies changed, added, or removed keys.

### Scanner Scope Validator (`scope_validator`)
Validates that a proposed patch only touches allowed files.
- **Allowed files:** `visualsettings.dat`, `cloudkeyframes.xml`, `timecycle_mods_1.xml`.
- **Blocked/Deferred:** `weather.xml`, `timecycle_mods_3.xml`, `timecycle_mods_4.xml`, `w_foggy.xml`, `w_clouds.xml`.
- **General blocks:** Binary files (`.ytd`, `.ypt`, etc.), RPF archives, and unrelated components (tracer, hit effects) are strictly blocked.

## CLI Usage

### Validate XML
```bash
rpf_backend_rs validate-xml --file <path> --baseline <baseline_path> --vmode color_like_only
```

### Validate DAT
```bash
rpf_backend_rs validate-dat --file <path> --baseline <baseline_path> --vmode allowed_family_only
```

### Validate Scope
```bash
rpf_backend_rs validate-scope --patch-plan <patch_plan.json> --changed-files file1.xml,file2.dat
```

## Limitations
- Validators prove syntax and scope, not visual correctness.
- In-game validation is still required.
- Color-like classification is conservative and may produce warnings.
- DAT key names do not prove exact visual meaning.
