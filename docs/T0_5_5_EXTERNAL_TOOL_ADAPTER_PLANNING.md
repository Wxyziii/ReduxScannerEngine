# T0.5.5 — External Tool Adapter Planning

This milestone adds **planning models** for future external RPF tooling adapters.
It only detects, describes, and plans external tool capability support — it never
invokes a tool to modify files, never writes an RPF archive, and never parses RPF
internals.

> **Safe-mode only.** Detection is an informational PATH lookup. No external tool
> is ever executed. `NullRpfAdapter` remains the active adapter. Real RPF writing
> and native RPF parsing are still **not** implemented.

## What `rpf-external-tools` does

1. Looks up known tools on `PATH` (informational only — nothing is executed):
   `OpenIV`, `CodeWalker`, `7z`, `powershell`, `cmd`.
2. For each tool, reports detection (`found` true/false + method), a trust level,
   a risk level, and the *theoretical* capabilities it might offer in a future,
   trusted integration.
3. Marks every archive-mutation and automatic-execution path as **blocked**.
4. Reports conservative adapter flags:
   - `externalToolsDetected`: true only if at least one tool is found on PATH
   - `canUseExternalToolsAutomatically`: `false`
   - `canModifyArchive`: `false`
   - `canWriteArchive`: `false`
   - `canParseInternals`: `false`
   - `safeModeOnly`: `true`
   - `manualUserActionRequired`: `true`

Missing tools never cause an error — a tool simply reports `found: false`.

## Detection is informational

- OpenIV and CodeWalker are GUI tools and are typically **not** on PATH; they
  will usually report not found. That is expected and harmless.
- No tool is executed (not even `--version`/`--help`).
- Detection never enables running a tool: every plan entry has `allowedNow: false`.

## Usage

```
rpf-external-tools [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- rpf-external-tools \
  --out .tmp/rpf_external_tools.json
```

Expected:

- the command exits `0`,
- known tools are listed with `found` true/false,
- `safeModeOnly` is `true`,
- `canWriteArchive` and `canUseExternalToolsAutomatically` are `false`,
- no files are modified (apart from the optional `--out` report).

## Integration with `rpf-adapter-info`

`rpf-adapter-info` now embeds the external-tool plan under `externalToolPlan`.
This is informational only:

- the active adapter is still `NullRpfAdapter`,
- `safeModeOnly` stays `true`,
- `canWriteArchive`, `canReplaceFiles`, `nativeParser`, and `nativeWriter` stay
  `false`.

The external adapter does **not** replace `NullRpfAdapter`.

## Future external write support

Before any external tool could ever be used to write a real archive, all of the
following must hold:

- a successful `backup-rpf` (hash-verified backup),
- a successful `probe-rpf` (read-only target metadata/hash),
- all `plan-rpf-write` safety gates satisfied,
- explicit manual user confirmation,
- an explicit trusted adapter mode (no tool is trusted today).

Until then, this layer is planning only.

## Test safety

All tests are pure in-memory planning checks plus temp-file writes for the
`--out` report and a do-not-modify-files assertion. No real GTA V files or
copyrighted archive content are used, no external tool is executed, and no
archive is opened or modified.
