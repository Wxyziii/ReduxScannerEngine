# T0.5.9 — CodeWalker Writer Strategy Lock-In

This milestone locks **CodeWalker.API** as the selected future writer route for
this project. It introduces `codewalker-strategy`: a static, deterministic report
command that records the decision and the planned path into T0.6.x.

> **No writing. No execution.** This milestone does not detect, call, or execute
> CodeWalker, does not implement RPF writing or native RPF parsing, and does not
> modify any RPF archive. `writerAllowed` and `codewalkerWriteAllowedNow` stay
> `false`. The active adapter remains `NullRpfAdapter`.

## The decision

- **Selected future writer route:** `CodeWalker.API`
  (`selectedWriterRouteLocked: true`).
- **Active adapter now:** `null_rpf_adapter` (`activeAdapterIsNull: true`).
- **Actual writing:** still disabled.
- **Planned default endpoint:** `http://localhost:5555` (planning value only —
  nothing connects to it in this milestone).

## What stays false this milestone

| field | value |
| --- | --- |
| `writerAllowedNow` | false |
| `codewalkerWriteAllowedNow` | false |
| `codewalkerDetectionImplemented` | false |
| `codewalkerExecutionImplemented` | false |
| `externalToolExecutionAllowed` | false |
| `realWriterImplemented` | false |
| `nativeParserImplemented` | false |

## Planned T0.6.x route

- **T0.6.0** — CodeWalker.API Detection Adapter (detect a local endpoint,
  informational only).
- **T0.6.1** — CodeWalker.API Readiness Probe.
- **T0.6.2** — CodeWalker Search/Resolve Plan.
- **T0.6.3** — CodeWalker Dry Replace Plan.
- **T0.6.4** — CodeWalker Replace Apply on a **copied test archive only**.
- **T0.6.5** — Post-write verification and rollback.

The progression is deliberate: detection → readiness → resolve → dry replace →
copied-test-archive execution → verify/rollback. No step mutates a real archive.

## Required safety gates (none satisfied yet)

`backup_rpf_verified`, `probe_rpf_successful`, `entry_manifest_built`,
`write_readiness_checked`, `writer_permission_token_present`,
`copied_test_archive_only`, `codewalker_api_detected`,
`codewalker_replace_endpoint_available`,
`codewalker_target_resolution_successful`, `manual_confirmation_required`,
`rollback_restore_available`, `post_write_verification_required`,
`codewalker_execution_not_enabled_yet`.

## Usage

```
codewalker-strategy [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- codewalker-strategy \
  --out .tmp/codewalker_strategy.json
```

Expected:

- exits `0`,
- `selectedWriterRoute` is `CodeWalker.API`, `selectedWriterRouteLocked` is true,
- `activeAdapterName` is `null_rpf_adapter`,
- `writerAllowedNow`, `codewalkerWriteAllowedNow`, `codewalkerDetectionImplemented`,
  `codewalkerExecutionImplemented`, and `externalToolExecutionAllowed` are all
  `false`,
- no files modified except optional `--out`.

## Future role

`codewalker-strategy` is the technical handoff into T0.6.x. **T0.6.0** will add
CodeWalker.API detection; later T0.6.x milestones move gradually from detection
to readiness to dry replace planning to copied-test-archive execution, with full
verification and rollback. Until those land — and until the safety gates above
pass — CodeWalker writing remains disabled and no archive mutation occurs.
