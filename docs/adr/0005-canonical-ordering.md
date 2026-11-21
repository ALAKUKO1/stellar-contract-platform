# ADR-0005: Canonical ordering for deterministic output

- **Status:** Accepted
- **Date:** 2026-01-01
- **Supersedes:** —
- **Superseded by:** —

## Context

Fingerprints and diffs must be stable and reproducible. A compiler may emit
declarations in any order, and JSON object key order is not semantically
meaningful, yet both can change bytes and produce spurious differences.

## Decision

Name-keyed collections (functions, structs, unions, enums, error enums, events)
are sorted by name before hashing or reporting. Sequence-keyed collections
(parameters, struct fields, union cases, enum cases) preserve their declared
order because that order is semantic for XDR encoding and the call ABI.
Fingerprints are computed over a deterministic canonical JSON encoding, and the
format is pinned by known-value vectors (see `docs/fingerprinting.md`).

## Consequences

- **Positive:** reordering declarations does not change the fingerprint; output
  is reproducible across builds and machines.
- **Negative:** canonicalization must be applied consistently before comparison.
- **Neutral:** changing the canonical encoding is a format change and requires
  updating the pinned vectors deliberately.
