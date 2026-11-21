# Fingerprinting

Fingerprints provide stable identity for artifacts and interfaces.

## What each fingerprint covers

| Fingerprint | Covers | Does not cover |
| ----------- | ------ | -------------- |
| Artifact SHA-256 | The exact WASM bytes | Interface meaning, source |
| Interface SHA-256 | The canonical interface (functions, types, events) | Implementation, metadata |

## Artifact fingerprint

`fingerprint <wasm>` prints the SHA-256 of the artifact bytes. This is the same
value used by `verify artifact`.

The CLI streams the file through a bounded buffer
(`observatory_wasm::artifact_fingerprint_file`) instead of loading it whole, so
peak memory stays constant regardless of artifact size. The digest is identical
to hashing the full contents at once. The interface fingerprint still needs the
artifact in memory; when an artifact exceeds the load bound it is omitted while
the artifact fingerprint is still reported.

## Interface fingerprint

When a `contractspecv0` section is present, the canonical interface is serialized
to deterministic JSON and hashed. Canonicalization sorts name-keyed collections
and preserves sequence-keyed collections, so:

- reordering function declarations does **not** change the fingerprint,
- reordering function parameters **does** change the fingerprint,
- changing a type does change the fingerprint.

A known-value test vector pins the digest of a small interface to detect
accidental format changes.

## Warning

```
same interface  !=  same implementation
```

Two contracts can expose an identical interface and behave differently. The
interface fingerprint captures the interface, not the behavior.
