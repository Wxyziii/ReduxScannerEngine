# T0.6.0 — CodeWalker.API Detection Adapter

This milestone adds a **safe, read-only** detection layer for a local
CodeWalker.API server. It introduces `codewalker-detect`: a reporting command
that checks whether a CodeWalker.API endpoint is reachable and whether its basic
read-only status endpoints respond.

> **No writing. No execution.** This milestone does not call replace/import/write
> or any mutation endpoint, does not execute CodeWalker as a process, does not
> implement RPF writing or native RPF parsing, and does not open or modify any
> RPF archive. The active adapter stays `NullRpfAdapter`. `writerAllowed` stays
> `false`.

## What it does

`codewalker-detect` performs only HTTP `GET` requests against the base URL
(default `http://localhost:5555`):

- `GET /` — root / Swagger UI presence,
- `GET /api/service-status` — read-only service status.

It records, for each probe, whether it was reachable and the HTTP status. If the
status endpoint answers, `codewalkerApiDetected` is `true`. `codewalkerReady`
stays `false` unless the status body clearly reports readiness.

If the server is **offline**, the command still returns a valid report with
`reachable: false` and exits `0` — detection failure is data, not an error.

## What it never does

- Never calls `/api/replace-file`, `/api/import`, or any write/mutation endpoint.
- Never calls `/api/reload-services` — even though that is not archive writing, it
  is a service mutation and is out of scope here.
- Never executes CodeWalker as a process.
- Never opens, reads, or writes an RPF archive.
- Never creates backups.
- Keeps `canReplaceFile`, `canImportFile`, `canWriteArchive`, and `writerAllowed`
  `false`.

## Implementation note

The HTTP probe is a tiny standard-library `TcpStream` GET (status line + body,
`Connection: close`, short timeout) — no new HTTP dependency was added. Tests use
a small in-process mock `TcpListener` server that records every requested path,
so they can prove `/api/replace-file` and `/api/import` were never called. Tests
require no real CodeWalker.API install.

## Always-false fields

| field | value |
| --- | --- |
| `canReplaceFile` | false |
| `canImportFile` | false |
| `canWriteArchive` | false |
| `writeEndpointsChecked` | false |
| `writeEndpointsCalled` | false |
| `replaceEndpointCalled` | false |
| `importEndpointCalled` | false |
| `externalToolExecuted` | false |
| `modifiesArchive` | false |
| `writerAllowed` | false |

## Usage

```
codewalker-detect [--base-url <url>] [--out <path>]
```

Examples:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- codewalker-detect

cargo run --manifest-path rpf_backend_rs/Cargo.toml -- \
  codewalker-detect --base-url http://localhost:5555 --out .tmp/codewalker_detect.json
```

Expected if CodeWalker.API is **not** running:

- exits `0`,
- `reachable` false, `serviceStatusAvailable` false,
- `canWriteArchive`, `writeEndpointsCalled`, `replaceEndpointCalled`,
  `modifiesArchive`, `writerAllowed` all false.

Expected if CodeWalker.API **is** running:

- exits `0`,
- `reachable` true, `serviceStatusAvailable` true if `/api/service-status`
  responds, `serviceStatusHttpStatus` recorded,
- `canWriteArchive`, `writeEndpointsCalled`, `replaceEndpointCalled`,
  `modifiesArchive`, `writerAllowed` still all false.

## Safety gates

`base_url_valid`, `readonly_detection_only`, `root_endpoint_checked`,
`service_status_endpoint_checked`, `write_endpoints_not_called`,
`replace_endpoint_not_called`, `external_tool_not_executed`,
`archive_not_modified`, `null_adapter_still_active`, `writer_allowed_false`.

## Next

**T0.6.1** will build a deeper CodeWalker.API readiness probe on top of this
detection layer. Until the full T0.6.x safety gates pass, CodeWalker writing
remains disabled and no archive mutation occurs.
