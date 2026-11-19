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

## Concepts kept separate

| Concept | Covered by |
| ------- | ---------- |
| Artifact identity | SHA-256 of the WASM bytes |
| Interface identity | canonical interface fingerprint |
| Build metadata equality | `verify compare` |
| Source reproducibility | **not implemented** (requires rebuilding) |

Do not collapse these. Equal metadata does not prove reproducibility, and equal
artifacts do not prove equal source.
