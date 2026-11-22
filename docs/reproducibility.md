# Reproducibility

Reproducibility is about whether an artifact can be rebuilt from source and
compared. This project does **not** reproduce builds; it reads and compares the
metadata a build embeds, so that a reproducibility decision can be made.

## Build metadata sources

The Soroban toolchain can embed metadata in custom sections:

- `contractenvmetav0` — environment metadata (`SCEnvMetaEntry`), including the
  target protocol and pre-release.
- `contractmetav0` — build metadata (`SCMetaEntry`) as key/value pairs, for
  example compiler or SDK versions.

`observatory-verify` parses both with the real XDR types. Unknown keys are
preserved verbatim, so the model stays extensible.

## Metadata vocabulary

Only keys with an authoritative definition are listed here. The SDK and CLI
treat `contractmetav0` as an open set of key/value strings, so the list is not
exhaustive by design.

### `contractenvmetav0`

The section is a serialized `SCEnvMetaEntry` union. It currently has one arm,
`SC_ENV_META_KIND_INTERFACE_VERSION`, whose `interfaceVersion` struct is what
`repro` reports as `env_meta`.

| Field | Meaning | Value type | Source |
| ----- | ------- | ---------- | ------ |
| `interfaceVersion.protocol` | Interface version the contract targets; it must match the host functions the contract was built against. | `u32` | [Stellar-contract-env-meta.x](https://github.com/stellar/stellar-xdr/blob/next/Stellar-contract-env-meta.x) |
| `interfaceVersion.preRelease` | Pre-release number for that interface version. | `u32` | [Stellar-contract-env-meta.x](https://github.com/stellar/stellar-xdr/blob/next/Stellar-contract-env-meta.x) |

### `contractmetav0`

The section is a stream of `SCMetaEntry` values. The only defined arm is
`SC_META_V0` (`SCMetaV0`), a `key` string and a `val` string; see
[Stellar-contract-meta.x](https://github.com/stellar/stellar-xdr/blob/next/Stellar-contract-meta.x).

| Key | Meaning | Value type | Source |
| --- | ------- | ---------- | ------ |
| `rsver` | Rust compiler version used to build the contract. | String, for example `1.85.0` | [soroban-sdk `lib.rs`](https://github.com/stellar/rs-soroban-sdk/blob/main/soroban-sdk/src/lib.rs) |
| `rssdkver` | `soroban-sdk` version. Builds may append `#<git-revision>` to identify the exact revision. | String, for example `22.0.0` or `22.0.0#abc123` | [soroban-sdk `lib.rs`](https://github.com/stellar/rs-soroban-sdk/blob/main/soroban-sdk/src/lib.rs) |
| `cliver` | Stellar CLI version. `stellar contract build` always injects this entry. | String, for example `22.8.1` | [stellar-cli build command](https://github.com/stellar/stellar-cli/blob/main/cmd/soroban-cli/src/commands/contract/build.rs) |
| `source_repo` | Source repository for the build, as `github:<owner>/<repo>`. | String | [SEP-0055](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0055.md) |
| `home_domain` | Domain that hosts the organization's SEP-0001 `stellar.toml`. No scheme or path is allowed. | String, for example `example.com` | [SEP-0055](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0055.md) |

### Unknown or not documented

Any key not in the table above has no standardized meaning here. This includes:

- Keys added by a contract with `soroban_sdk::contractmeta!`.
- Keys passed to `stellar contract build --meta <key>=<value>`.
- Keys that tooling or a particular project happens to use.

The Stellar documentation names examples of what developers may embed (contract
name, version, author, supported interfaces, source repository, home domain) but
defines no fixed key names for them, and the `contractenvmetav0` union is
extensible. `repro` therefore reports unknown keys verbatim as key/value pairs
and never interprets them, infers a meaning from the name, or drops them. A key
appearing in an artifact is not evidence that the key is standardized.

## Reproducibility report

`repro <WASM> [--reference <WASM>]` reports:

- `env_meta` — parsed `contractenvmetav0`, or `null`,
- `build_metadata` — parsed `contractmetav0` key/value pairs, or `null`,
- `comparison.state` — `not_compared`, `match`, or `mismatch`,
- `comparison.env_meta_equal` and `comparison.build_metadata` — per-field
  comparison against the reference, when one is supplied,
- `source_reproduction` — always `not_performed`.

The command reads embedded metadata only. It does not rebuild the artifact, so
the report cannot and does not claim source reproducibility.

## Comparing metadata

`verify compare` reports:

- `equal` — whether all keys agree,
- `compared_keys` — the size of the union of keys,
- `differences` — per-key `left`/`right` values (absent keys are `null`).

## Dependency metadata

`repro --dependencies [--reference <WASM>]` reports dependency entries recorded
in `contractmetav0` under keys of the form `dep:<name>`. The value is the
version string, kept verbatim; an empty value is reported as `null`.

The `dep:` namespace is an Observatory convention, not an upstream Stellar
standard. Keys outside it are never interpreted as dependencies; they remain in
the raw `build_metadata` map and are reported verbatim by `repro`.

When no `dep:` key is present, `dependencies` is `null` and the comparison
state is `unavailable`. Absence is never reported as an empty dependency list.
With `--reference`, the union of dependency names is compared; differences are
sorted by name, so identical inputs always produce identical output. A
comparison is `unavailable` unless both artifacts record dependency metadata.

## Minimal SBOM

`repro <WASM> --sbom` emits a minimal, honest SBOM document. It is **not** SPDX
or CycloneDX; the format identifier is `observatory-sbom/1`.

- `component.name` — the `name` metadata key, or `null`.
- `component.version` — the `version` metadata key, or `null`.
- `component.hashes` — SHA-256 of the artifact bytes, always present.
- `dependencies` — `dep:<name>` entries, or `null` when none were recorded.
- `unavailable` — field paths that could not be determined.

The `name` and `version` keys are convenience fields, not an upstream Stellar
standard; their absence is reported as `null` and listed in `unavailable`, never
filled in. The SBOM never invents dependency data: entries come only from
`dep:` keys, and `dependencies` is `null` when the artifact carries none.

## Concepts kept separate

| Concept | Covered by |
| ------- | ---------- |
| Artifact identity | SHA-256 of the WASM bytes |
| Interface identity | canonical interface fingerprint |
| Build metadata equality | `verify compare` |
| Source reproducibility | **not implemented** (requires rebuilding) |

Do not collapse these. Equal metadata does not prove reproducibility, and equal
artifacts do not prove equal source.
