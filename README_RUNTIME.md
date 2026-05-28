# Redux RPF Scanner - Runtime README

Redux RPF Scanner is a read-only GTA V RPF archive scanner. It inspects clean or modded archives, compares manifests, and writes JSON artifacts for local analysis and HomeOps automation.

## Commands

- `version` - Prints the scanner version for quick health checks.
- `validate-tools` - Verifies backend availability and key file status.
- `baseline-scan` - Scans a clean archive and writes baseline artifacts for later comparisons.
- `diff-against-baseline` - Compares a modded archive to a stored baseline; supports `--analyze-text` and `--build-learning-corpus`.
- `classify-rpf` - Classifies an unknown archive against a known baseline.
- `scan-rpf` - Legacy single-archive scan that writes one manifest JSON.
- `compare-rpf` - Legacy two-archive compare that writes one comparison JSON.

## Required dist layout

```text
redux_rpf_scanner(.exe)
tools/rpf_backend_rs(.exe)
rules/component_rules.json   (optional; falls back to built-in)
rules/target_rules.json      (optional; falls back to built-in)
```

## Notes

- A `keys` directory is required for encrypted archives; never put keys in this folder or Git.
- The scanner is read-only and does not modify archives.
- Generated output artifacts such as `learning_corpus/` and diff output should stay local.
- See `docs/HOMEOPS_SCANNER_CONTRACT.md` for HomeOps integration details.
