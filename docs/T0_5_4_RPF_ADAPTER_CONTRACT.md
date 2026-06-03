# T0.5.4 — RPF Adapter Contract

This milestone defines a **safe adapter contract** for future RPF read/write
implementations. It does **not** implement any real RPF parsing or writing — it
only fixes the trait shape, capability model, unsupported-operation behavior,
and safety expectations that every future adapter must honor.

> **Safe-mode only.** The single adapter that exists today is `NullRpfAdapter`.
> It never opens, parses, or modifies an archive, and never invokes external
> tools. Real RPF writing and native RPF parsing are still **not** implemented.

## The contract

```rust
trait RpfAdapter {
    fn name(&self) -> &'static str;
    fn kind(&self) -> RpfAdapterKind;
    fn capabilities(&self) -> RpfAdapterCapabilities;
    fn plan_operation(&self, operation: RpfAdapterOperation) -> RpfAdapterOperationPlan;
    fn execute_operation(&self, operation: RpfAdapterOperation) -> RpfAdapterOperationResult;
}
```

`execute_operation` MUST NOT modify anything in this milestone. It exists to
lock in the shape of the contract so future native or external-tool adapters
can be slotted in without changing callers.

### Operations

`probe_metadata`, `list_entries`, `extract_file`, `replace_file`,
`write_archive`.

### Capabilities (current `NullRpfAdapter` values)

| Capability | Value | Notes |
|---|---|---|
| `canProbeMetadata` | `true` | Only via the existing read-only probe layer (`probe-rpf`). |
| `canListEntries` | `false` | No internal parsing. |
| `canExtractFiles` | `false` | No internal parsing. |
| `canReplaceFiles` | `false` | No writing. |
| `canWriteArchive` | `false` | No writing. |
| `requiresExternalTool` | `false` | The null adapter calls nothing. |
| `nativeParser` | `false` | Not implemented. |
| `nativeWriter` | `false` | Not implemented. |
| `safeModeOnly` | `true` | Always true for the null adapter. |

## `NullRpfAdapter`

The default adapter is safe-mode only. It:

- exposes the conservative capability set above,
- returns **blocked / not implemented** for `list_entries`, `extract_file`,
  `replace_file`, and `write_archive`,
- never opens or modifies archives,
- never calls external tools,
- reports the block type `native_rpf_adapter_not_implemented` for every
  unsupported operation.

`probe_metadata` is the only "supported" operation, and only in the sense that
metadata/hash is available read-only through `probe-rpf`; the adapter itself
still does not open the archive.

## Usage

```
rpf-adapter-info [--out <path>]
```

Example:

```
cargo run --manifest-path rpf_backend_rs/Cargo.toml -- rpf-adapter-info \
  --out .tmp/rpf_adapter_info.json
```

Expected:

- the command exits `0`,
- `safeModeOnly` is `true`,
- `canWriteArchive`, `canReplaceFiles`, `nativeParser`, `nativeWriter` are all
  `false`,
- `nativeAdapterImplemented` and `modifiesArchive` are `false`,
- no files are modified.

## Relationship to the other RPF layers

- `probe-rpf` — read-only metadata/hash of a single archive (T0.5.2).
- `compare-rpf` — read-only metadata/hash comparison of two archives (T0.5.3).
- `rpf-adapter-info` — reports the adapter contract and capabilities (this
  milestone). The current adapter cannot parse or write archive internals.
- `plan-rpf-write` now includes a `rpf_adapter_supports_write` safety gate that
  fails because the active adapter is the safe-mode `NullRpfAdapter`. `safeToWrite`
  remains `false` and the terminal `real_rpf_writer_not_implemented` gate still
  blocks.

## Future adapters

A future adapter may be **native** (in-process RPF implementation) or
**external-tool** based (driving something like OpenIV/CodeWalker). Either way
it must:

- implement the same `RpfAdapter` contract,
- report honest capabilities,
- and still pass every existing safety gate (backup, restore, hash
  verification, manual confirmation, and adapter-supports-write) before any real
  write can occur.

## Test safety

Adapter tests are pure in-memory contract checks plus a temp-file write for the
`--out` report. No real GTA V files or copyrighted archive content are used, and
no archive is opened or modified.
