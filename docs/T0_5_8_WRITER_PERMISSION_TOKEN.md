# T0.5.8 — Writer Permission Token / Manual Confirmation Schema

This milestone models the **final manual confirmation object** required before
any future controlled RPF write. It introduces `writer-permission`: a read-only
command that validates the inputs and an exact confirmation phrase and may issue
a planning **permission token**.

> **Read-only.** `writer-permission` reads existing reports and checks an exact
> confirmation phrase. It never opens or modifies the target RPF, never modifies
> the bundle, never creates backups, and never executes external tools. The
> permission token does **not** enable writing: `writerAllowed` is always
> `false`.

## What the permission report answers

- Did the user explicitly confirm the target RPF path? (supplied `--target-rpf`
  plus the exact confirmation phrase)
- Did the user explicitly confirm they understand backup/restore risk? (the
  token records `confirmedBackupRequired` / `confirmedRestoreRequired` /
  `confirmedHashVerificationRequired`)
- Was a verified backup report provided? (`--backup-report`, validated for
  `hashVerified` + `safeForFutureWrite`)
- Was a readiness report provided? (`--readiness-report`, validated for
  `readyToWrite == false`)
- Was an entry manifest provided? (`--entry-manifest-report`, validated for
  `readyForWrite == false`)
- Is `writerAllowed` false because the writer is still not implemented? **Yes.**
- What would be needed later to turn this into a real write authorization?
  A real RPF writer, native RPF parsing, and a write-capable adapter — all of
  which are still blocked (see the blocking items below).

## The confirmation phrase

The user must pass `--confirm` with this exact phrase:

```
I understand this is planning-only and does not write the RPF
```

A missing phrase fails `confirmation_phrase_provided`; a wrong phrase fails
`confirmation_phrase_matched`. Both prevent token issuance.

## When a token is issued

A planning token is generated only when **all** of the following hold:

- the bundle directory exists,
- the target path exists and has a `.rpf` extension,
- every *provided* report (readiness / entry manifest / backup) is valid,
- the confirmation phrase matches exactly.

Even then, the token records `writerAllowed: false` and
`readyToWriteAtCreation: false`, and the report still lists the terminal
blocking items:

- `real_rpf_writer_not_implemented`
- `native_rpf_parser_not_implemented`
- `active_adapter_cannot_write`

## Gates

| gate | severity | meaning |
| --- | --- | --- |
| `bundle_dir_present` | blocking | `--bundle-dir` exists |
| `target_rpf_present` | blocking | `--target-rpf` exists |
| `target_rpf_extension_valid` | blocking | target ends in `.rpf` |
| `readiness_report_present_or_missing` | info/blocking | provided report is found |
| `readiness_report_valid_if_present` | info/blocking | `readyToWrite == false` + target matches |
| `entry_manifest_report_present_or_missing` | info/blocking | provided report is found |
| `entry_manifest_valid_if_present` | info/blocking | `readyForWrite == false` + target matches |
| `backup_report_present_or_missing` | info/blocking | provided report is found |
| `backup_report_hash_verified_if_present` | info/blocking | `hashVerified` + `safeForFutureWrite` + target matches |
| `confirmation_phrase_provided` | blocking | `--confirm` supplied |
| `confirmation_phrase_matched` | blocking | phrase matches exactly |
| `real_rpf_writer_implemented` | blocking | always false this milestone |
| `native_rpf_parser_implemented` | blocking | always false this milestone |
| `adapter_supports_write` | blocking | NullRpfAdapter cannot write |
| `writer_permission_allowed` | blocking | always false this milestone |

## Usage

```
writer-permission --bundle-dir <path> --target-rpf <path>
                  [--readiness-report <path>] [--entry-manifest-report <path>]
                  [--backup-report <path>] --confirm "<phrase>" [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- writer-permission \
  --bundle-dir .tmp/redux_patch_bundle_permission_test \
  --target-rpf examples/rpf_fixtures/fake_update.rpf \
  --readiness-report .tmp/write_readiness_permission_report.json \
  --entry-manifest-report .tmp/rpf_entry_manifest_permission_report.json \
  --backup-report .tmp/rpf_backup_permission_report.json \
  --confirm "I understand this is planning-only and does not write the RPF" \
  --out .tmp/writer_permission_report.json
```

Expected:

- exits `0` as a reporting command,
- a permission token is present when the inputs are valid and the phrase matches,
- `writerAllowed`, `modifiesTargetArchive`, `realWriterImplemented`, and
  `nativeParserImplemented` are all `false`,
- blocking items still include writer/parser/adapter blockers,
- the target RPF and the bundle are unchanged; no external tool is executed.

## Future role

This is the schema for the **final authorization gate** before controlled
writing. In a later milestone — once a real RPF writer, native RPF parsing, and
a write-capable adapter exist — the same confirmation token will be the last step
that flips authorization on. Until then, producing a token is purely a planning
exercise and never permits a write.

## Test safety

All tests build tiny fixtures in temp directories and use the fake `.rpf`
fixture as the (never-opened) target. No real GTA V files or copyrighted archive
content are used, no archive is opened or modified, and no external tool is
executed.
