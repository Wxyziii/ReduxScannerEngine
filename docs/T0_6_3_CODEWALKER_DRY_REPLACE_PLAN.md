# T0.6.3 — CodeWalker Dry Replace Plan

## Summary

`codewalker-dry-replace-plan` produces a **dry replace plan**: a structured,
read-only description of exactly what a future writer would send to CodeWalker.API
to replace files inside an RPF archive. It combines four local inputs:

- the **exported bundle** files (`<bundle>/files/...`)
- the **RPF entry manifest report** (T0.5.7) — archive-relative paths + SHA-256
- the **CodeWalker resolve report** (T0.6.2) — resolved/ambiguous/unresolved targets
- an **optional writer-permission report** (T0.5.8)

For each manifest entry it finds the matching resolved CodeWalker target, verifies
the providing bundle file exists, computes its SHA-256, compares that to the
manifest hash, and—only when everything lines up—emits a **modelled**
`/api/replace-file` payload. The payload is never sent anywhere.

## This command is local and read-only

`codewalker-dry-replace-plan`:

- reads only local bundle/manifest/resolve (and optional permission) report files
- creates planned replace payloads for **future** CodeWalker execution
- **does not send any HTTP request** (neither GET nor POST)
- **does not use POST**
- **does not call** `/api/replace-file`
- **does not call** `/api/import`, `/api/reload-services`, or `/api/set-config`
- **does not call** any mutation endpoint
- **does not execute** CodeWalker as a process
- **does not execute** any external tool
- **does not open or modify** any RPF archive
- **does not modify** the bundle, staged, or source workspace files
  (only the optional `--out` JSON is written)

`readyForExecution` remains **false**, `writerAllowed` remains **false**, and
`codewalkerExecutionAllowed` remains **false** — even when every item is valid.
The active adapter stays `NullRpfAdapter`.

## Usage

```
codewalker-dry-replace-plan --bundle-dir <path> --entry-manifest-report <path>
        --resolve-report <path> [--permission-report <path>] [--out <out.json>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-dry-replace-plan \
  --bundle-dir .tmp/redux_patch_bundle_entry_manifest_test \
  --entry-manifest-report .tmp/rpf_entry_manifest_report.json \
  --resolve-report .tmp/codewalker_resolve_targets.json \
  --out .tmp/codewalker_dry_replace_plan.json
```

The command always exits `0` as a reporting command.

## Output

The report (`CodeWalkerDryReplacePlanReport`) includes:

- `status`: `planned` (all items valid), `partial` (some valid), `blocked`
  (none valid), or `invalid_input` (a required input was unusable)
- `plannedRequests`: the modelled `/api/replace-file` payloads
- `plannedEndpoint` = `/api/replace-file`, `plannedHttpMethod` = `POST`
- `items`: per-entry detail (resolved path, bundle file existence/size/SHA-256,
  hash-match, match type, planned payload, validity, block reason)
- `safetyGates`, `blockedItems`, `warnings`, `summary`
- conservative safety flags: `dryRunOnly: true`, `readyForExecution: false`,
  `writerAllowed: false`, `codewalkerExecutionAllowed: false`,
  `postRequestsSent: false`, `getRequestsSent: false`, `replaceEndpointCalled: false`,
  `importEndpointCalled: false`, `reloadServicesCalled: false`,
  `setConfigCalled: false`, `mutationEndpointsCalled: false`,
  `externalToolExecuted: false`, `modifiesArchive: false`,
  `realWriterImplemented: false`, `nativeParserImplemented: false`

### Item-level blocking

A blocked item never fails the whole report. An item is blocked (no payload,
`validForFutureReplace: false`) when its CodeWalker target is unresolved or
ambiguous, the bundle file is missing, the manifest lacks a SHA-256, or the bundle
file hash does not match the manifest.

## Next milestone

**T0.6.4** will prepare copied-test-archive execution gating: applying a replace
strictly on a COPIED test archive, never on a real game archive, behind explicit
manual confirmation. Real RPF writing and native RPF parsing remain unimplemented.
