# Verification

Verification compares a local artifact against a deployment. It is explicit about
what is being compared.

## Artifact verification

`verify artifact` compares the SHA-256 of the local artifact against the deployed
executable hash.

```bash
stellar-contract-platform verify artifact contract.wasm \
  --contract C... --rpc-fixture rpc.json --network testnet
```

| Status | Meaning | Exit code |
| ------ | ------- | --------- |
| `MATCH` | Local hash equals the deployed executable hash. | `0` |
| `MISMATCH` | Hashes differ. | `7` |
| `INSUFFICIENT_DATA` | No instance entry, or the contract is not WASM-backed. | `8` |
| `ERROR` | The comparison could not be completed. | `8` |

Those are the default exit codes. To change which statuses fail, set
`ci.fail_on` in the project configuration to a comma-separated list of
`MISMATCH`, `INSUFFICIENT_DATA`, and `ERROR`:

```yaml
ci:
  fail_on: MISMATCH
```

The default is all three statuses. A status left out of the list exits `0`;
for example, `fail_on: MISMATCH` fails on a mismatch but lets an inconclusive
comparison pass. `MATCH` is not accepted, because a match never fails a build.
The same policy applies to `verify interface`.

### What this proves, and what it does not

Matching hashes prove that the local and deployed **artifacts** are identical.
They do **not** prove that the artifact was built reproducibly from a given
source tree. Source-level reproducibility is a separate problem; see
[reproducibility.md](reproducibility.md). The tool never claims source-level
verification from a hash comparison.

## Interface verification

`verify interface` compares the canonical interface of the local artifact with
the interface of the deployed contract code (fetched by hash). It reports
`interface_match` as `true`, `false`, or `null` when unknown (the local artifact
has no specification, or the deployed code could not be fetched or parsed).

When both interfaces are known, the report also includes:

- `diff` — the interface diff with the local interface as the baseline,
- `compatibility` — the strict compatibility verdict for the deployed interface
  relative to the local one.

Artifact identity and interface identity are always reported separately: the
artifact status is the hash comparison, while `interface_match` is the interface
comparison. `source_verification` is always `not_performed`; an interface match
is not source verification. Exit codes: `0` when the interfaces match, `7` when
they differ, `8` when unknown.

The comparison is transport-independent: `verify_interface_with_source` accepts
a `DeployedCodeSource`, which tests use to inject match, mismatch, and missing
code. Live fetching remains blocked on issue #14; the shipped CLI reads deployed
code only through deterministic RPC fixtures.

```bash
stellar-contract-platform verify interface contract.wasm \
  --contract C... --rpc-fixture rpc.json --network testnet --json
```

## Build metadata

`verify metadata <file>` parses a build metadata JSON document and prints its
key/value pairs.

`verify compare <left> <right>` compares two documents and reports the union of
differing keys:

```bash
stellar-contract-platform verify compare build-a.json build-b.json --json
```

The comparison reports `equal`, `compared_keys`, and `differences`.
