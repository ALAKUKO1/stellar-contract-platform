# Troubleshooting

## `error: cannot read ...: The system cannot find the file specified`

The path passed to a command does not exist. The CLI exits `3` for input errors.

## `error: wasm error: input does not start with the WASM magic bytes`

The file is not a WASM module. Confirm you are pointing at a `.wasm` artifact.

## `error: spec error: WASM module has no 'contractspecv0' section`

The module is WASM but has no Soroban contract specification. It may be a plain
library, or built without the Soroban contract macro.

## `spec` shows `unknown type` validation warnings

A type referenced by a function or field is not declared in the specification.
This usually indicates a build issue or a hand-crafted fixture.

## `compat` exits `6`

The change is incompatible under the active policy. Inspect the reasons, or use
`--policy lenient` if your consumers tolerate removals (deliberately).

## `compat` returns `unknown`

The baseline interface is empty. There is no public surface to compare, so the
tool refuses to claim compatibility.

## `verify artifact` exits `8`

The deployment data was insufficient: no instance entry, or the contract is not
WASM-backed. Confirm the contract id and the RPC fixture.

## `deployment` says the live transport is unavailable

This build ships only a deterministic fixture transport. Provide
`--rpc-fixture`. A live HTTP transport is tracked as future work.

## JSON mode prints an error object

With `--json`, errors are emitted as `{ "kind": "error", "code": ..., "message":
... }` on stdout, and the process exits with the corresponding code.

## Error codes

Every failure carries a stable machine-readable `ErrorCode` in the JSON `code`
field and maps to a process exit code (see the [CLI exit-code
table](cli.md#exit-codes)). The table below maps each code to its typical causes
and remedies.

| `ErrorCode` | Exit | One-line summary |
| ----------- | ---- | ---------------- |
| `invalid_input` | `2` | The command was invoked incorrectly |
| `io` | `3` | A file or directory could not be read or written |
| `decode` | `4` | Input was well-formed but could not be decoded |
| `unsupported` | `5` | A valid input uses a feature this build does not support |
| `wasm` | `4` | The WASM module is malformed or invalid |
| `spec` | `4` | The contract specification is malformed or invalid |
| `rpc` | `3` | An RPC transport or protocol failure occurred |
| `verification` | `8` | Verification failed or could not be completed |
| `internal` | `1` | An internal invariant was violated |

### `invalid_input` (exit `2`)

**Cause:** Missing or conflicting arguments, an unknown policy name, or a value
that fails validation before any I/O happens. Clap also uses exit `2` for usage
errors.

**Remedy:** Re-run with `--help` and correct the invocation. Check that paths
and required options are present.

### `io` (exit `3`)

**Cause:** A file, directory, or fixture path does not exist or is not readable,
or an output path cannot be written.

**Remedy:** Confirm the path and permissions. On Windows, verify the file is not
locked by another process. See also the "cannot read" entry above.

### `decode` (exit `4`)

**Cause:** A document (for example build metadata, an event fixture, or a JSON
report) was read successfully but does not match the expected encoding or
schema.

**Remedy:** Validate the document against the relevant schema in
[`docs/schemas`](schemas/README.md) and regenerate it from the producing tool.

### `unsupported` (exit `5`)

**Cause:** The input is well-formed but uses a feature this build does not
implement, such as an unknown specification entry kind or a newer XDR version.

**Remedy:** Upgrade the tool. If the feature is genuinely new, file an issue
with the decoded prefix reported in the error message.

### `wasm` (exit `4`)

**Cause:** The module does not start with the WASM magic bytes, is truncated, or
fails structural validation.

**Remedy:** Point at a real `.wasm` artifact and rebuild it if it is corrupt. See
the "input does not start with the WASM magic bytes" entry above.

### `spec` (exit `4`)

**Cause:** The module is valid WASM but the `contractspecv0` section is missing,
truncated, or internally inconsistent (for example a reference to an undeclared
type).

**Remedy:** Rebuild with the Soroban contract macro so the specification section
is emitted, and fix unknown type references flagged by `spec` validation.

### `rpc` (exit `3`)

**Cause:** The RPC fixture is malformed, or a (future) live transport failed to
connect or returned a JSON-RPC error. The typed `RpcError` preserves the raw
code and classifies standard JSON-RPC codes.

**Remedy:** Regenerate the fixture, or inspect the preserved raw code. See
[`docs/rpc-errors.md`](rpc-errors.md) for the classification table.

### `verification` (exit `8`)

**Cause:** Verification could not be completed with the available data (no
instance entry, a non-WASM-backed contract) or was inconclusive. A definitive
mismatch uses exit `7` instead.

**Remedy:** Confirm the contract id, the RPC fixture, and the network. Re-run
`verify artifact` against a fixture that includes the contract instance.

### `internal` (exit `1`)

**Cause:** An invariant that should be impossible was violated. This indicates a
bug, not a problem with your input.

**Remedy:** Re-run with `-v` to capture diagnostics and report the issue with
the command, input fixture, and backtrace.
