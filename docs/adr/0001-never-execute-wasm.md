# ADR-0001: Never execute untrusted WASM

- **Status:** Accepted
- **Date:** 2026-01-01
- **Supersedes:** —
- **Superseded by:** —

## Context

The tool inspects artifacts supplied by users. Executing a supplied WASM module
would run untrusted code with the privileges of the analysis process, exposing
the host to arbitrary code execution, resource exhaustion, and sandbox escapes.
We only need structural facts (sections, hashes, specifications), not runtime
behaviour.

## Decision

The Observatory never executes WASM. Inspection is purely static and uses the
bounded streaming parser in `wasmparser`. Contract specifications are decoded
from the `contractspecv0` custom section and parsed, never instantiated.

This is enforced at the crate boundary: `observatory-wasm` exposes
parse/inspect functions only, and there is no runtime or interpreter dependency
in the workspace.

## Consequences

- **Positive:** untrusted input cannot execute; the attack surface is a parser.
- **Negative:** behaviour that is only observable at runtime cannot be checked.
- **Neutral:** any future feature must preserve the no-execution invariant; see
  ADR-0004 for the bounds that keep parsing safe.
