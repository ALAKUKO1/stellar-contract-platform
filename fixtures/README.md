# Interoperability fixtures

These artifacts are real WASM modules produced by the Soroban SDK, not modules
assembled from XDR or WASM primitives. They are checked in so inspection and
specification extraction can be tested without a Rust-to-WASM toolchain or
network access.

## Provenance

Both contracts were written for this repository and built locally from the
sources checked in beside each artifact. They are **not** deployed ecosystem
contracts, and no claim is made that they represent a third party's build.

| Artifact | Source | SHA-256 |
| -------- | ------ | ------- |
| `soroban/asset-token/asset-token.wasm` | `soroban/asset-token/src/lib.rs` | `74eac831f2538c6c75ab800711feb44e262590e608a0049fa9176eae559b3452` |
| `soroban/counter/counter.wasm` | `soroban/counter/src/lib.rs` | `45f052e74f290618215f6982e08c25fd9c0652568d4723e98a595b8eb04eee47` |

Build environment and command:

- `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, host `x86_64-pc-windows-gnu`
- `cargo 1.99.1 (5f94df478 2026-08-27)`
- target `wasm32-unknown-unknown`
- `soroban-sdk = "=21.7.7"` (locked in each fixture's `Cargo.lock`)
- built on 2026-11-22 with
  `cargo build --target wasm32-unknown-unknown --release` from each fixture
  directory; the release profile is declared in the fixture's `Cargo.toml`

The SHA-256 values record what is checked in. Tests assert stable summaries
(names, counts, and types), not these hashes, because a toolchain or SDK change
can legitimately change the bytes.

## Licensing

The fixture sources are part of this repository and are licensed under
Apache-2.0, the same as the rest of the project. See [`../../LICENSE`](../../LICENSE).

## Rebuilding

Each fixture is a standalone Cargo workspace so it never becomes a member of the
main workspace:

```text
cd fixtures/soroban/asset-token
cargo build --target wasm32-unknown-unknown --release
```

Copy the resulting `asset_token_fixture.wasm` over `asset-token.wasm`, rebuild
the counter the same way, then update the hashes above. Regenerating is only
needed when the fixture source or SDK version changes; the tests do not rebuild
at test time.

## Tests

`crates/observatory-cli/tests/ecosystem.rs` inspects both artifacts, extracts
their specifications, and builds canonical interfaces from them.
