# T0.6.1 — CodeWalker.API Readiness Probe

This milestone adds a deeper **read-only** readiness probe for CodeWalker.API. It
introduces `codewalker-readiness`: a reporting command that decides whether the
API service appears usable for future search/replace **planning** — without
calling any write/mutation endpoint.

> **No writing. No mutation. No execution.** This milestone uses only HTTP `GET`.
> It never calls replace/import/reload-services/set-config or any mutation/POST
> endpoint, never executes CodeWalker as a process, does not implement RPF writing
> or native RPF parsing, and never opens or modifies an RPF archive. The active
> adapter stays `NullRpfAdapter`. `writerAllowed` stays `false`.

## What it does

`codewalker-readiness` builds on `codewalker-detect`:

1. Runs `detect_codewalker_api` (read-only `GET /` + `GET /api/service-status`).
2. If reachable, does **one** extra safe `GET /api/service-status` to capture the
   raw body and tolerantly parse it.

From the parsed status it extracts, best-effort, any of: `ready` / `status`,
`servicesReady`, GTA path, reload/api version. An unexpected JSON shape never
fails the probe — fields stay unknown and `serviceStatusJsonParseSuccess` records
the outcome. A non-JSON body is captured raw and flagged with a warning.

`codewalkerApiReadyForSearch` becomes `true` only when the status **clearly**
reports readiness (e.g. `ready: true`, `servicesReady: true`, or a ready/ok/
online/running status). `codewalkerApiReadyForReplace` stays `false` this
milestone.

If the server is **offline**, the command still returns a valid not-ready report
with `reachable: false` and exits `0`.

## What it never does

- Never calls `/api/replace-file`, `/api/import`, `/api/reload-services`,
  `/api/set-config`, or any other mutation endpoint.
- Never issues a POST (or any non-GET) request.
- Never executes CodeWalker as a process.
- Never opens, reads, or writes an RPF archive.
- Never creates backups.

## Always-false fields

| field | value |
| --- | --- |
| `codewalkerApiReadyForReplace` | false |
| `canCallReplaceLater` | false |
| `canWriteArchive` | false |
| `writeEndpointsCalled` | false |
| `replaceEndpointCalled` | false |
| `importEndpointCalled` | false |
| `reloadServicesCalled` | false |
| `setConfigCalled` | false |
| `mutationEndpointsCalled` | false |
| `postRequestsUsed` | false |
| `externalToolExecuted` | false |
| `modifiesArchive` | false |
| `writerAllowed` | false |

## Usage

```
codewalker-readiness [--base-url <url>] [--out <path>]
```

Examples:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- codewalker-readiness

cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-readiness --base-url http://localhost:5555 --out .tmp/codewalker_readiness.json
```

Expected if CodeWalker.API is **not** running:

- exits `0`,
- `codewalkerApiReachable` false, `codewalkerApiReadyForSearch` false,
  `codewalkerApiReadyForReplace` false,
- `canWriteArchive`, `writerAllowed`, and every mutation/write flag false.

Expected if CodeWalker.API **is** running and status is ready:

- exits `0`,
- `codewalkerApiReachable` true, `serviceStatusHttpStatus` recorded, status
  parsed/raw captured,
- `codewalkerApiReadyForSearch` true if the status clearly reports readiness,
- `codewalkerApiReadyForReplace`, `canWriteArchive`, `writerAllowed` still false,
- no mutation endpoint called.

## Safety gates

`detection_report_built`, `codewalker_api_reachable`,
`service_status_endpoint_checked`, `service_status_parse_attempted`,
`readonly_get_only`, `no_post_requests_used`, `reload_services_not_called`,
`set_config_not_called`, `write_endpoints_not_called`,
`replace_endpoint_not_called`, `import_endpoint_not_called`,
`mutation_endpoints_not_called`, `null_adapter_still_active`,
`writer_allowed_false`, `archive_not_modified`.

## Next

**T0.6.2** will add CodeWalker search / target-resolution planning on top of this
readiness layer. Until the full T0.6.x safety gates pass, CodeWalker writing
remains disabled and no archive mutation occurs.
