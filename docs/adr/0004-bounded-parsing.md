# ADR-0004: Bounded parsing of all inputs

- **Status:** Accepted
- **Date:** 2026-01-01
- **Supersedes:** —
- **Superseded by:** —

## Context

Artifacts, specification sections, events, and HTTP bodies are attacker-
controlled. An unbounded parser can be forced to allocate or loop without limit,
turning inspection into a denial-of-service vector. We need predictable resource
use regardless of input.

## Decision

Every input is parsed under explicit limits. `observatory-core::limits` defines
the shared ceilings (artifact size, custom-section size, entry counts, depth,
header counts, body size). Parsers reject oversized or malformed input with a
typed error rather than truncating silently. Section dumps are bounded (64 KiB)
and large payloads are rejected.

## Consequences

- **Positive:** bounded memory and time; malformed input fails closed; limits
  are testable and documented.
- **Negative:** legitimate inputs above a limit are rejected instead of handled.
- **Neutral:** new parsers must consume the shared limits; do not introduce
  per-module constants that bypass them.
