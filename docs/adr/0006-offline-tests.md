# ADR-0006: Offline, deterministic tests

- **Status:** Accepted
- **Date:** 2026-01-01
- **Supersedes:** —
- **Superseded by:** —

## Context

Tests that depend on the wall clock, randomness, or the network are flaky and
hide regressions behind environmental differences. The project also promises
reproducible reports, which requires reproducible test inputs.

## Decision

No unit or integration test touches the network. Fixtures are built
deterministically from shared builders in `observatory-testutil`, property tests
use a fixed seed, and time-dependent behaviour is injected rather than read from
the clock. Network-facing behaviour is exercised through the fixture transport
(ADR-0003).

## Consequences

- **Positive:** tests run anywhere without credentials; failures are
  reproducible; CI is fast and hermetic.
- **Negative:** some real-world integration paths are only covered indirectly.
- **Neutral:** any test that would need the network must instead add a fixture.
