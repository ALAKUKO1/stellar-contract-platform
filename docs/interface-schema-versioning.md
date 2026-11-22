# Interface schema versioning

`observatory_interface::INTERFACE_SCHEMA_VERSION` identifies the layout and
canonicalization rules of the serialized `ContractInterface` (the JSON document
produced by `canonical_json()` and hashed by `fingerprint()`). It is the
contract between this crate and anything that persists, exchanges, or compares
canonical interface documents.

The current value is `1.0`. The version tracks the **representation**, not the
Soroban specification version and not the compatibility of two contracts.

## Additive vs. breaking changes

An **additive** change keeps every valid `1.0` document valid and readable by a
consumer that already understands `1.0`, and leaves existing canonical bytes
unchanged. Additive changes do not change the schema version. Examples:

- A new helper method or accessor on `ContractInterface`.
- A new diff `ChangeKind` or report field that consumers may ignore.
- Documentation or internal refactors that do not alter canonical output.
- A new optional field with a default that serializes identically for existing
  interfaces.

A **breaking** change alters either the set of fields, their meaning, or the
canonical byte encoding, such that existing documents or fingerprints would be
misread or silently change. Breaking changes require a schema-version bump and
a changelog entry. Examples:

- Adding, removing, or renaming a field of `ContractInterface` or any
  `Spec*` model type that it serializes.
- Changing canonicalization: sort order, normalization, whitespace, key
  ordering, or type-tag encoding.
- Changing which entities participate in a fingerprint.
- Changing the interpretation of an existing field without changing its name.

When in doubt, treat a change as breaking. A fingerprint that silently changes
is indistinguishable from an interface that changed, which defeats the purpose
of deterministic comparison.

## Consumer guidance

- **Check the version first.** Read `schema_version` from a persisted document
  or report envelope. Do not assume the current constant matches archived data.
- **Reject or quarantine unknown versions.** If the document's version is
  newer than the consumer supports, fail closed or store it without interpreting
  it. Do not best-effort parse a newer schema.
- **Pin what you persist.** Store the `schema_version` alongside every
  fingerprint. A fingerprint without its schema version is ambiguous.
- **Do not compare fingerprints across schema versions.** Recompute the
  fingerprint with the matching crate version after a bump.
- **Treat additive fields as optional.** Consumers must ignore fields they do
  not understand rather than rejecting the whole document.
- **Keep compatibility decisions out of the schema.** The schema describes
  structure; consult `observatory-compat` for whether a change is breaking to
  callers.

## Changelog requirement

Every change to `INTERFACE_SCHEMA_VERSION` **must** be recorded in
[`CHANGELOG.md`](../CHANGELOG.md) under `Unreleased`, stating:

1. the old and new version,
2. whether the change is additive or breaking,
3. what representation or canonicalization changed, and
4. what consumers must do to migrate (for example, recompute fingerprints).

A pull request that bumps the constant without a changelog entry is incomplete.
Reviewers should treat a version bump and its changelog entry as one unit.

## Version history

| Version | Change |
| ------- | ------ |
| `1.0` | Initial canonical interface representation: sorted functions, structs, unions, enums, error enums, and events; ordered parameters, fields, and cases. |
