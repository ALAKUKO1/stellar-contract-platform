# ADR-0002: Separate description from judgement

- **Status:** Accepted
- **Date:** 2026-01-01
- **Supersedes:** —
- **Superseded by:** —

## Context

Comparing two contract interfaces produces two different questions: *what
changed* and *whether that change is acceptable*. Mixing them makes reports
harder to test, reason about, and re-use under different compatibility policies.
Different consumers also have different tolerances for change.

## Decision

`observatory-diff` enumerates concrete changes and classifies each as breaking,
non-breaking, or informational. `observatory-compat` then applies an explicit
policy (`strict`, `lenient`, or a policy file) to turn those changes into a
verdict. Compatibility is never a bare boolean and is never computed inside the
diff layer.

## Consequences

- **Positive:** the diff is policy-agnostic and reusable; policies are testable
  in isolation; reports explain *why* a verdict was reached.
- **Negative:** callers must combine two layers to reach a final answer.
- **Neutral:** new policies extend `observatory-compat` without touching the
  diff algorithm.
