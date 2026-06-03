# T0.6.4 — CodeWalker Copied-Test-Archive Execution Gate

## Summary

`codewalker-execution-gate` answers a single question:

> *"Would we even **allow** a future CodeWalker replace attempt against this
> target archive?"*

It produces a strict, read-only **execution-gate report**. It decides
**eligibility only** — it never executes CodeWalker and never modifies any
archive. Even when every gate passes, **no execution happens in this milestone.**

The only target class that can ever be eligible is a **copied test archive**:

- explicitly marked/confirmed as a test copy (`--target-is-test-copy`)
- not the original game archive
- not inside a detected original game install path
- backed up and hash-verified
- backed by a dry replace plan, a permission token, a write readiness report,
  and an entry manifest

## This command is local and read-only

`codewalker-execution-gate`:

- reads only the local target fixture and the five local report files
- decides whether a **future** CodeWalker replace attempt is eligible
- only allows **copied-test-archive** eligibility
- **blocks original game install paths** (conservative path detection)
- **does not send any HTTP request** (neither GET nor POST)
- **does not use POST**
- **does not call** `/api/replace-file`
- **does not call** `/api/import`, `/api/reload-services`, or `/api/set-config`
- **does not call** any mutation endpoint
- **does not execute** CodeWalker as a process
- **does not execute** any external tool
- **does not open or modify** any RPF archive
- **does not create backups**
- **does not modify** any report, bundle, staged, or source workspace file
  (only the optional `--out` JSON is written)

`codewalkerExecutionEligible` **may be true**. But:

- `codewalkerExecutionAllowedNow` remains **false**
- `codewalkerExecutionPerformed` remains **false**
- `writerAllowed` remains **false**
- `modifiesArchive` remains **false**

The active adapter stays `NullRpfAdapter`.

## Target classification

`targetArchiveClassification` is one of:

- `copied_test_archive` — confirmed test copy, not in a game path (**eligible**)
- `original_game_archive_suspected` — path looks like an original install (**blocked**)
- `unknown_archive` — not confirmed as a test copy (**blocked**)
- `missing` — the target file does not exist (**blocked**)
- `invalid_extension` — the target is not a `.rpf` file (**blocked**)

Original-install path detection is intentionally conservative. A path is treated
as an original install when it contains (case-insensitively) any of:

- `Grand Theft Auto V`
- `GTA V`
- `steamapps/common`
- `Epic Games/GTAV`
- `Rockstar Games/Grand Theft Auto V`

or ends with `/update/update.rpf` under a game-like path.

## Input reports

The gate reads (and tolerantly validates) five prior-phase reports:

| Input | Requirement to pass its strict gates |
|-------|--------------------------------------|
| Dry replace plan (T0.6.3) | parses; `plannedRequests` > 0; `dryRunOnly: true`; no replace/POST recorded. `readyForExecution: false` is expected and accepted. |
| Permission report (T0.5.8) | parses; permission token present; `confirmationPhraseMatched: true`. `writerAllowed: false` is expected and accepted. |
| Readiness report (T0.5.6) | parses. `readyToWrite: false` is expected and accepted. |
| Entry manifest report (T0.5.7) | parses; has at least one entry. `readyForWrite: false` is expected and accepted. |
| Backup report (T0.5.1) | parses; `hashVerified: true`; `safeForFutureWrite: true`; if a target path is present it matches `--target-rpf`. |

## Usage

```
codewalker-execution-gate --target-rpf <path> --dry-replace-plan <path>
        --permission-report <path> --readiness-report <path>
        --entry-manifest-report <path> --backup-report <path>
        --target-is-test-copy [--out <out.json>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-execution-gate \
  --target-rpf examples/rpf_fixtures/fake_update.rpf \
  --dry-replace-plan .tmp/codewalker_dry_replace_plan.json \
  --permission-report .tmp/writer_permission_report.json \
  --readiness-report .tmp/write_readiness_report.json \
  --entry-manifest-report .tmp/rpf_entry_manifest_report.json \
  --backup-report .tmp/rpf_backup_report.json \
  --target-is-test-copy \
  --out .tmp/codewalker_execution_gate.json
```

The command always exits `0` as a reporting command.

## Output

The report (`CodeWalkerExecutionGateReport`) includes:

- `status`: `eligible` (all strict gates pass), `blocked` (a strict gate failed),
  or `invalid_input` (a required report/target was unusable)
- `targetArchiveClassification` and `targetPathAllowedForTestExecution`
- per-report status (`valid`/`invalid`/`unparsable`/`missing`) and `*Valid` flags
- `gates`: each with `name`, `passed`, `severity` (`info`/`warning`/`blocking`),
  and `message`
- `warnings`, `blockedItems`, `summary`
- conservative safety flags: `codewalkerExecutionEligible` (may be `true`),
  `codewalkerExecutionAllowedNow: false`, `codewalkerExecutionPerformed: false`,
  `writerAllowed: false`, `modifiesArchive: false`, `replaceEndpointCalled: false`,
  `importEndpointCalled: false`, `reloadServicesCalled: false`,
  `setConfigCalled: false`, `postRequestsSent: false`, `httpRequestsSent: false`,
  `externalToolExecuted: false`, `realWriterImplemented: false`,
  `nativeParserImplemented: false`

## Next milestone

**T0.6.5** will be the first **controlled execution design**, still targeting
**copied test archives only**, behind explicit manual confirmation and post-write
verification/rollback. Real RPF writing and native RPF parsing remain
unimplemented.
