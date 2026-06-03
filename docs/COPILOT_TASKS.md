# Copilot Task Prompts

Use these as focused prompts for GitHub Copilot / coding agents.

---

## Active Phase: T0 — Controlled Patch Framework

### Task T0.4.2 — XML Mutation Logic
```text
Implement safe XML mutation for timecycle and cloudkeyframe files.
- Use `quick-xml` or similar to parse and write.
- Target: `cloudkeyframes.xml` and `timecycle_mods_1.xml`.
- Implement node/value replacement based on `EditorOperation`.
- MUST use `XmlValidator` before and after to ensure integrity.
- MUST NOT delete nodes or change non-color numeric values.
- Follow the T0.4.1 Editor Contract.
- Read-only until instructed otherwise.
```

### Task T0.4.3 — DAT Mutation Logic
```text
Implement safe DAT mutation for `visualsettings.dat`.
- Support named-key replacement (key value).
- Target: `visualsettings.dat`.
- MUST use `DatValidator` before and after.
- Only allow `Adaptation` and `Tonemapping` family changes.
- Ensure structure and comments are preserved if possible.
```

---

## Completed Tasks

- [x] **Task 1: Version and validate-tools.** Build and CLI foundation.
- [x] **Task 2: Structured warnings and schema metadata.** Schema v2.0.
- [x] **Task 3: Scan Modes.** Fast, Targeted, Deep, Full modes.
- [x] **Task 4: Baseline Scan.** Clean archive fingerprinting.
- [x] **Task 5: RPF Classifier.** logical-name fallback detection.
- [x] **Task 6: Unknown Pattern Discovery.** R0.6 artifacts.
- [x] **Task 7: Text Analyzers.** Inside-file comparison.
- [x] **Task 8: Learning Corpus.** AI-ready context.
- [x] **Task 9: Timecycle Intelligence.** Strategic reports.
- [x] **Task T0.1-T0.4.1:** Controlled Patch safety framework.

---

## Legacy Task Prompts (Reference)

### Task 1 — Version and validate-tools
... (existing content)
