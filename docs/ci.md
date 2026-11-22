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

By default a mismatch exits `7` and an inconclusive or errored comparison exits
`8`. To accept inconclusive results without accepting a mismatch, set:

```yaml
ci:
  fail_on: MISMATCH
```

`fail_on` is a comma-separated list of `MISMATCH`, `INSUFFICIENT_DATA`, and
`ERROR`; statuses left out exit `0`. See
[verification.md](verification.md#artifact-verification).

## Baseline-vs-head compatibility job

Run this check on pull requests against the committed fixtures in
`fixtures/compat/` with the strict policy:

```bash
./target/debug/stellar-contract-platform compat \
  fixtures/compat/baseline.wasm fixtures/compat/head.wasm --policy strict
```

The fixtures contain only a `contractspecv0` section, so the job does not build a
Soroban contract and needs no WASM target. An incompatible change to
`fixtures/compat/head.wasm` exits `6` and fails the job.

Run the committed negative pair
(`fixtures/compat/negative/baseline.wasm` vs `negative/head.wasm`) as well and
assert that it reports `incompatible` with exit code `6`; that step exercises the
failure path on every run, even when the gate pair is compatible.
See [`../fixtures/compat/README.md`](../fixtures/compat/README.md) for the fixture
contents.

The GitHub Actions definition for this job is supplied in the pull request that
added the fixtures and is not committed here; commit it as
`.github/workflows/compat.yml` to enable the check.

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

## Dependency deny policy

`deny.toml` runs `cargo-deny` on every pull request (see the
`security-audit.yml` workflow, job `cargo-deny`) and checks advisories, licenses,
duplicate/wildcard bans, and crate sources.

- **Advisories**: yanked crates are denied; a vulnerability is fixed by updating
  the lockfile, not by ignoring it.
- **Licenses**: only common permissive/public-domain licenses and the
  weak-copyleft licenses already present (MPL-2.0) are allowed, each listed in
  `deny.toml`. Add a license only with a documented reason in review.
- **Bans**: wildcard dependencies are denied; duplicate versions are warnings so
  they can be reduced deliberately.
- **Sources**: only crates.io is allowed; unknown registries and git sources are
  denied.

To reproduce locally:

```bash
cargo install cargo-deny --locked
cargo deny check
```
