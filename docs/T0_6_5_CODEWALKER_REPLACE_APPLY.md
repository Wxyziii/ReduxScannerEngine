# T0.6.5 — Controlled CodeWalker Replace Apply on Copied Test Archive

## Summary

`codewalker-replace-apply` is the **first scoped CodeWalker replace executor**. It
is the first milestone that may issue a CodeWalker replace HTTP request — and it
is heavily gated. It sends only `POST /api/replace-file`, and only when **every**
authorization gate passes.

It runs only when **all** of these hold:

- the T0.6.4 **execution gate report** is loaded and `codewalkerExecutionEligible`
  is `true`
- the gate classified the target as a **copied test archive**
  (`copied_test_archive`)
- the target archive **exists**
- the T0.6.3 **dry replace plan** is loaded, was `dryRunOnly`, and has at least
  one planned request
- the explicit `--execute` flag is present
- the exact confirmation phrase matches:
  `I understand this will call CodeWalker replace on a copied test archive`
- the base URL is a usable `http(s)` URL

If any blocking gate fails, **no HTTP request is sent** and a blocked report is
returned.

## Scope and hard limits

`codewalker-replace-apply`:

- is intended **only for copied test archives** — never an original game archive
- sends **only** `POST /api/replace-file`
- **does not call** `/api/import`, `/api/reload-services`, `/api/set-config`, or
  the search endpoint (`/api/search-file`)
- **does not execute** CodeWalker as a process
- **does not execute** any external tool
- **does not parse** RPF internals (no native parser)
- **does not roll back / restore** — rollback is not implemented in this milestone
- **does not create backups** (run `backup-rpf` beforehand)
- **does not modify** source/staged workspace files (only the optional `--out`
  JSON is written)

Original game archives are blocked **upstream** by the execution gate (T0.6.4),
which classifies original-install paths as `original_game_archive_suspected` and
refuses eligibility. This command never targets a path the gate did not approve.

## Global writer stays disabled

This command performs a **scoped** execution only. Regardless of outcome:

- global `writerAllowed` remains **false**
- the active adapter remains `NullRpfAdapter`
- `nativeParserUsed` and `nativeWriterUsed` remain **false**

`executionScopedWriterAllowed` reflects only that this single gated command was
permitted to send its requests; it does not change the global writer state.

## Request payload

Each request is a conservative, fully visible JSON body (the exact CodeWalker.API
shape may evolve and is recorded in the report for auditability):

```json
{
  "rpfPath": "<resolved CodeWalker path>",
  "archivePath": "<resolved CodeWalker path>",
  "sourceFilePath": "<bundle file absolute path>",
  "archiveRelativePath": "<archive-relative path>",
  "dryRunOnly": false,
  "execute": true
}
```

`modifiesArchive` is `true` only when at least one request was actually sent and
returned success (HTTP 2xx). The command also records the target SHA-256 before
and after (when the file is locally accessible) and reports `targetHashChanged`
as `changed` / `unchanged` / `unknown`.

## Usage

```
codewalker-replace-apply --base-url <url> --execution-gate-report <path>
        --dry-replace-plan <path> --execute --confirm "<phrase>" [--out <out.json>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-replace-apply \
  --base-url http://localhost:5555 \
  --execution-gate-report .tmp/codewalker_execution_gate.json \
  --dry-replace-plan .tmp/codewalker_dry_replace_plan.json \
  --execute \
  --confirm "I understand this will call CodeWalker replace on a copied test archive" \
  --out .tmp/codewalker_replace_apply.json
```

The command always exits `0` as a reporting command.

- Without `--execute`, a wrong/missing `--confirm`, or an ineligible gate: **no
  HTTP request is sent**, `status: blocked` (or `invalid_input`).
- With a reachable server returning success: `POST /api/replace-file` is sent,
  `status: executed`, request/response recorded.
- With the server offline: the request is attempted and fails cleanly
  (`status: failed`), no other endpoint is called, and the target hash is
  unchanged.

## Output

The report (`CodeWalkerReplaceApplyReport`) includes:

- `status`: `executed` / `partially_executed` / `failed` / `blocked` /
  `invalid_input`
- authorization facts: `executeRequested`, `confirmationPhraseMatched`,
  `executionGateEligible`, `copiedTestArchiveConfirmed`
- `itemResults`: per-request `request` (method/url/endpoint/payload body) and
  `response` (status/succeeded/body summary/error)
- hash audit: `originalTargetSha256`, `postExecutionTargetSha256`,
  `targetHashChanged`
- `gates`, `warnings`, `blockedItems`, `summary`
- endpoint-isolation flags (all `false`): `importEndpointCalled`,
  `reloadServicesCalled`, `setConfigCalled`, `searchEndpointCalled`,
  `externalToolExecuted`, `nativeParserUsed`, `nativeWriterUsed`,
  `rollbackPerformed`; plus global `writerAllowed: false`

## Real CodeWalker manual test (optional)

Only after automated tests pass, and only against a **copied test archive** —
never an original `update.rpf`:

- copy the target first, e.g.
  `C:\Users\Marcel\Downloads\ReduxScannerTest\test-copy\update.rpf`
- run `backup-rpf` on the copy first; keep the backup and reports
- never target a clean/original input directly

## Next milestone

**T0.6.6** will add **post-write verification and rollback planning** for copied
test archives: verifying the written output and planning a restore on mismatch.
Native RPF parsing and global RPF writing remain unimplemented.
