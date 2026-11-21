# Architecture Decision Records

This directory records the significant architectural decisions behind the
Observatory. Each record is immutable once accepted: a later decision that
supersedes an earlier one is added as a new record that links back to it.

## Records

| ADR | Title | Status |
| --- | ----- | ------ |
| [0001](0001-never-execute-wasm.md) | Never execute untrusted WASM | Accepted |
| [0002](0002-description-vs-judgement.md) | Separate description from judgement | Accepted |
| [0003](0003-fixture-first-rpc.md) | Fixture-first RPC transport | Accepted |
| [0004](0004-bounded-parsing.md) | Bounded parsing of all inputs | Accepted |
| [0005](0005-canonical-ordering.md) | Canonical ordering for deterministic output | Accepted |
| [0006](0006-offline-tests.md) | Offline, deterministic tests | Accepted |

## Adding a record

1. Copy [`template.md`](template.md) to the next free number, e.g.
   `0007-my-decision.md`.
2. Fill in the context, decision, and consequences.
3. Set the status to `Proposed`, then `Accepted` once agreed.
4. Add a row to the table above.
5. Link the record from [`../architecture.md`](../architecture.md).

Statuses used: `Proposed`, `Accepted`, `Superseded by ADR-NNNN`, `Deprecated`.
