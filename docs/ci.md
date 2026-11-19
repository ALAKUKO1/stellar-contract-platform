# CI integration

The Observatory is designed to run in CI: deterministic output, stable JSON
envelopes, and meaningful exit codes.

## Exit codes for gating

| Code | Use |
| ---- | --- |
| `0` | pass |
| `6` | incompatible interface change (`compat`) |
| `7` | verification mismatch (`verify artifact`) |
| `8` | verification inconclusive |

## Example: detect breaking interface changes

```yaml
name: Interface compatibility
on: pull_request
jobs:
  compat:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
      - uses: dtolnay/rust-toolchain@stable
      - name: Build
        run: cargo build --release
      - name: Build baseline
        run: |
          git worktree add baseline origin/${{ github.base_ref }}
          cargo build --release --manifest-path baseline/Cargo.toml || true
      - name: Compare interfaces
        run: |
          ./target/release/stellar-contract-platform compat \
            baseline/target/release/contract.wasm \
            contract.wasm --json > compat.json
```

## Example: verify a released artifact

```yaml
      - name: Verify artifact
        run: |
          ./target/release/stellar-contract-platform verify artifact \
            contract.wasm --contract C... --rpc-fixture ci/rpc.json --network testnet
```

## Coverage

The `Coverage` CI job runs `cargo-llvm-cov` on pull requests and pushes to
`main`, writes a summary to the job summary, and uploads an `lcov.info` artifact.
The coverage floor is advisory (`continue-on-error`) while a baseline is
established. See [testing.md](testing.md#coverage) for the exact commands.

## JSON reports

Every command supports `--json`. Reports are wrapped in a versioned envelope:

```json
{ "schema_version": "1.0", "kind": "compat", "tool": "...", "version": "...", "data": {} }
```

Automation should pin `schema_version` and treat unknown `kind` values as errors.

## GitHub Action

The composite action lives at `.github/actions/observatory`. It installs the CLI
and runs `inspect`, `diff`, or `compat`, writing a JSON report. It is a
deterministic analysis helper, not a security audit or certification.

| Input | Default | Meaning |
| ----- | ------- | ------- |
| `command` | `inspect` | `inspect`, `diff`, or `compat`. |
| `wasm` | required | Artifact to inspect, or the updated artifact for `diff`/`compat`. |
| `baseline` | empty | Baseline artifact; required for `diff` and `compat`. |
| `report` | `observatory-report.json` | Path for the JSON report. |
| `security` | `false` | Pass `--security` to `inspect`. |
| `repository` | `Tech-Cred/stellar-contract-platform` | Repository to install the CLI from. |
| `ref` | `main` | Git ref, tag, or commit to install. |
| `path` | empty | Local `observatory-cli` crate path; skips the git install. |

Outputs: `exit-code` (the CLI exit code) and `report` (the written path). A Rust
toolchain is required; add `dtolnay/rust-toolchain` before the action.

```yaml
name: Contract checks
on: pull_request
jobs:
  observatory:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
      - name: Inspect the artifact
        uses: Tech-Cred/stellar-contract-platform/.github/actions/observatory@main
        with:
          command: inspect
          wasm: contract.wasm
          security: "true"
          report: observatory-inspect.json
      - name: Compare against the baseline
        uses: Tech-Cred/stellar-contract-platform/.github/actions/observatory@main
        with:
          command: compat
          baseline: baseline.wasm
          wasm: contract.wasm
          report: observatory-compat.json
```

`compat` exits `6` for an incompatible change, so the step fails the job. Use
`continue-on-error: true` on that step when the report is advisory, or inspect
`steps.<id>.outputs.exit-code` to gate explicitly.

The repository workflow `.github/workflows/observatory-action.yml` exercises all
three commands against the demo fixtures.
