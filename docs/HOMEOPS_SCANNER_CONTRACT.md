# HomeOps Scanner Contract

## Purpose

Describes how HomeOps calls the scanner as a background job.

## Scanner invocation model

- HomeOps invokes: `redux_rpf_scanner(.exe) <command> <flags>`
- Scanner writes artifacts to `--out` folder
- Scanner exits `0` on success, non-zero on failure
- HomeOps reads artifacts after process exits
- Scanner does not require interactive input
- Scanner does not call LLM APIs
- Scanner is read-only for input archives

## Command reference table

| Command | Required flags | Optional flags | Output | Notes |
|---------|---------------|---------------|--------|-------|
| `version` | (none) | (none) | prints `redux_rpf_scanner <version>` to stdout | use for health check; no file output |
| `validate-tools` | `--keys <keys_dir>` | `--backend <path>` | JSON to stdout (`ok`, backend version, key file status) | exit `0` if all ok, exit `1` if any issue |
| `baseline-scan` | `--archive <clean.rpf>`, `--keys <keys_dir>`, `--out <baseline_dir>` | `--depth N` (default `2`), `--mode full`, `--component-rules`, `--target-rules`, `--rules-dir` | folder artifacts: `full_clean_manifest.json`, `full_clean_tree.json`, `baseline_update_tree_fingerprint.json`, `baseline_metadata.json` | run once per clean archive; store `baseline_dir` somewhere persistent |
| `diff-against-baseline` | `--modded <modded.rpf>`, `--baseline <baseline_dir>`, `--keys <keys_dir>`, `--out <diff_dir>` | `--depth N`, `--mode full`, `--clean <clean.rpf>` (required for `--analyze-text` on old baselines), `--analyze-text`, `--build-learning-corpus`, `--component-rules`, `--target-rules`, `--rules-dir` | folder artifacts: `full_modded_manifest.json`, `full_modded_tree.json`, `clean_vs_modded_diff.json`, `diff_summary.json`, `unknown_changes.json`, `unknown_text_candidates.json`, `unknown_binary_candidates.json`, `candidate_patterns.json`, `llm_review_queue.jsonl`, `unknown_summary.json`; if `--analyze-text`: `text_analysis_summary.json`, `xml_diffs.json`, `dat_diffs.json`, `meta_diffs.json`, `generic_text_diffs.json`, `analyzer_warnings.json`, `ai_readable_change_notes.jsonl`; if `--build-learning-corpus`: `learning_corpus/` subfolder with 10 artifacts | primary HomeOps compare command |
| `classify-rpf` | `--archive <unknown.rpf>`, `--baseline <baseline_dir>`, `--keys <keys_dir>`, `--out <output.json>` | `--depth N` (default `3`) | `classification.json` at `--out` path | classifies unknown RPF as `obvious_update_rpf` / `likely_update_rpf` / `possible_update_rpf` / `not_update_rpf` / `unknown_rpf` / `scan_failed` |
| `scan-rpf` | `--archive <update.rpf>`, `--keys <keys_dir>`, `--out <manifest.json>` | `--backend <path>`, `--depth N`, `--mode fast|targeted|deep|full`, `--all`, `--targets-only`, `--component-rules`, `--target-rules`, `--rules-dir` | manifest JSON at `--out` path | legacy single-archive scan; `baseline-scan` is preferred |
| `compare-rpf` | `--clean <clean.rpf>`, `--modded <modded.rpf>`, `--keys <keys_dir>`, `--out <report.json>` | `--backend <path>`, `--depth N`, `--mode fast|targeted|deep|full`, `--all`, `--targets-only`, `--component-rules`, `--target-rules`, `--rules-dir` | compare JSON at `--out` path | legacy two-archive compare; `diff-against-baseline` is preferred |

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success; artifacts written to `--out` |
| 1 | General failure (missing keys, missing archive, invalid baseline, parse error, write error) |
| 127 | Process spawn failure (backend not found or not executable) |

Common failure categories:
- Missing keys directory → exit `1`, `failed to load keys directory` in stderr
- Missing or unreadable archive → exit `1`, `failed to open archive` in stderr
- Invalid baseline (missing `baseline_metadata.json`) → exit `1`, `failed to read baseline` in stderr
- Output write failed → exit `1`, `failed to write` in stderr
- Backend not found → exit `127` from launcher

## Stdout / stderr contract

- stdout: progress lines and final summary (human-readable)
- stderr: errors only
- HomeOps should rely on artifacts in `--out` folder, not parse stdout
- stdout ends with `SCANNER_OK <out_path>` on success for each main scan command (see note below)

Note: `SCANNER_OK` line is printed by the backend after writing all artifacts. HomeOps may grep for this line as a secondary health signal.

## Output path safety

- Scanner creates `--out` directory if it does not exist (including parent dirs)
- Scanner overwrites its own generated artifacts (safe; idempotent reruns)
- Scanner never modifies files outside `--out` (except temp files cleaned up automatically)
- Scanner never modifies input archives

## Keys contract

- Keys must be stored outside Git and outside the scanner dist folder
- Keys are passed at runtime via `--keys <dir>`
- Required key files:
  - `gtav_aes_key.dat`
  - `gtav_ng_key.dat`
  - `gtav_ng_decrypt_tables.dat`

## Security rules

- Never store keys in the scanner package
- Never store `.rpf` files in the scanner package
- Never commit generated corpus/diff artifacts to Git
- Scanner does not extract or commit game assets

## Recommended HomeOps workflow

```bash
# Step 1: one-time baseline (run when clean archive changes)
redux_rpf_scanner baseline-scan \
  --archive /data/clean/update.rpf \
  --keys /secrets/rpf_keys \
  --out /data/baseline \
  --mode full

# Step 2: diff each Redux candidate
redux_rpf_scanner diff-against-baseline \
  --modded /data/redux/update.rpf \
  --baseline /data/baseline \
  --keys /secrets/rpf_keys \
  --out /data/diffs/redux_v2 \
  --mode full \
  --analyze-text \
  --build-learning-corpus

# Step 3: check exit code
# Step 4: read /data/diffs/redux_v2/diff_summary.json
# Step 5: optionally read learning_corpus/ artifacts
```
