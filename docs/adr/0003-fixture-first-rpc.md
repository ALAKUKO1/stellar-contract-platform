# ADR-0003: Fixture-first RPC transport

- **Status:** Accepted
- **Date:** 2026-01-01
- **Supersedes:** —
- **Superseded by:** —

## Context

Deployment inspection and verification need ledger data from an RPC endpoint.
Tests that depend on a live network are flaky, slow, and require credentials.
Separating the transport from the protocol also lets us ship a usable build
before a reviewed live transport exists.

## Decision

The RPC layer is transport-agnostic: `observatory-rpc` defines the protocol
types and a deterministic fixture transport, and deployment/verification code
depends only on the transport abstraction. The shipped build supports
`--rpc-fixture`; a live HTTPS transport is validated and recorded but not
enabled until SSRF protections land (see `docs/architecture.md`).

## Consequences

- **Positive:** all tests are hermetic; the protocol layer is independently
  testable; adding a live transport does not change callers.
- **Negative:** the shipped build cannot read live chain state yet.
- **Neutral:** a future live transport must implement the same abstraction and
  add SSRF protections before it is enabled.
