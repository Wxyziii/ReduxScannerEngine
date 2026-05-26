# Copilot Task Prompts

Use these as focused prompts for GitHub Copilot / coding agents.

---

## Task 1 — Version and validate-tools

```text
Inspect the repo first.

Implement only Scanner v0.2 Phase 1:
- Add `version` command to the C++ launcher.
- Add `version` command to the Rust backend.
- Add `validate-tools` command to the C++ launcher.
- Add backend support if needed.
- `validate-tools --keys <keys_dir>` should output JSON.
- It should check:
  - backend path exists
  - keys folder exists
  - gtav_aes_key.dat exists
  - gtav_ng_key.dat exists
  - gtav_ng_decrypt_tables.dat exists
- Create output parent folders automatically for scan/compare.
- Preserve existing scan-rpf and compare-rpf behavior.
- Update README command docs.
- Run cargo check and C++ build if possible.
Do not implement scan modes, cache, rules files, AI, UI, archive importer, or file editing.
```

---

## Task 2 — Structured warnings and schema metadata

```text
Implement only Scanner v0.2 Phase 2:
- Add schemaVersion to scan and compare JSON.
- Add tool metadata:
  - scanner name
  - scanner version
  - backend name
  - backend version
  - platform
- Convert warnings from Vec<String> to structured warning objects:
  - code
  - severity
  - path
  - message
- Preserve backward compatibility as much as practical.
- Update docs/sample schema notes.
Do not change scan logic or component rules yet.
```

---

## Task 3 — Rich compare fields

```text
Implement only rich compare output:
- sizeDelta
- sizeDeltaPercent
- extension
- category
- components
- editorNeeded
- risk
- likelyPattern
- confidence
- warning if exact meaning requires a deeper analyzer
Keep existing allChanges fields available if possible.
Do not add cache or rules file yet.
```

---

## Task 4 — Scan modes

```text
Add scan modes:
- fast
- targeted
- deep
- full

Map old flags:
- --targets-only should behave like targeted
- --all should behave like full

Update C++ launcher and Rust backend.
Keep default behavior close to current targeted mode.
```

---

## Task 5 — Component rules file

```text
Move component/target rules into JSON:
- rules/component_rules.example.json
- rules/target_rules.example.json

Add --rules <path> support.
Keep built-in fallback rules.
Do not break old behavior if rules file is missing.
```
