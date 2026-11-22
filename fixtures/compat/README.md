# Interface compatibility fixtures

These WASM modules are small, committed artifacts used by the
`.github/workflows/compat.yml` job. They carry only a `contractspecv0` custom
section, so the compatibility check runs without a Soroban or Rust-to-WASM
toolchain.

They are assembled from the same XDR spec entries that
`observatory-testutil::module_with_spec` builds in tests; they are not deployed
contracts.

| Pair | Baseline | Head | Expected |
| ---- | -------- | ---- | -------- |
| Gate | `baseline.wasm` | `head.wasm` | compatible (`total_supply` added) |
| Negative | `negative/baseline.wasm` | `negative/head.wasm` | incompatible (`transfer` removed) |

The gate pair is what a pull request compares: editing `head.wasm` to remove or
change a public entry makes the workflow fail with exit code `6`. The negative
pair exists so the workflow also proves its own failure path on every run.
