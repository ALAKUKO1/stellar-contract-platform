# Contributing to Stellar Contract Observatory

Thanks for helping build contract intelligence tooling for Stellar and Soroban.

## Ground rules

- Be respectful; see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- Never commit secrets, keys, or credentials.
- Do not claim a feature works unless it is implemented and tested.
- Core inspection must work offline and must never execute untrusted contract code.
- Prefer composing with official crates (`stellar-xdr`, `stellar-strkey`) over
  reimplementing Stellar primitives.

## Setup

```bash
git clone https://github.com/AyinkxLab/stellar-contract-platform
cd stellar-contract-platform
cargo build
cargo test
```

## Minimum supported Rust version (MSRV)

The workspace MSRV is Rust **1.88** (`rust-version` in the workspace
`Cargo.toml`). CI has a dedicated `MSRV (1.88.0)` job that runs on pull requests
and pushes to `main`. It checks every resolved dependency's declared
`rust-version`, then runs `cargo build --workspace --locked` and
`cargo test --workspace --all-features --locked` with Rust 1.88.0. The job fails
with an explicit message when the workspace or a dependency needs a newer rustc.

Do not raise the MSRV to work around a dependency. Pin the dependency instead:

```bash
cargo update -p <crate> --precise <version>
cargo metadata --format-version 1 --locked
```

Confirm no package declares `rust_version` above `1.88.0`, and keep the MSRV
job's toolchain and the workspace `rust-version` in sync when the MSRV is raised
deliberately.

## Before opening a pull request

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Architecture

Read [docs/architecture.md](docs/architecture.md). Crate boundaries are
meaningful: lower-level crates must not depend on higher-level ones. The CLI
composes the libraries; libraries never depend on the CLI.

## Adding functionality

- Every fallible function returns `observatory_core::ObservatoryError`.
- Parsing untrusted input must be bounded and must return structured errors.
- Add deterministic tests; do not require network access in unit tests.
- Decoding changes must include malformed-input tests.
- Update the relevant document under `docs/` and `CHANGELOG.md` for
  user-visible changes.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat`, `fix`,
`refactor`, `test`, `docs`, `ci`, `security`, `perf`, `build`, `chore`.

## Definition of done

- Compiles with clippy `-D warnings`.
- `cargo fmt` is clean.
- Tests added and passing.
- Documentation updated.
- No secrets, placeholders, or fake implementations.

## License

Contributions are licensed under Apache-2.0.
