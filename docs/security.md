# Security

## Model

Stellar Contract Observatory treats every artifact, specification, event, and RPC
response as untrusted input. The core guarantees:

- **No execution.** WASM is never run. Inspection is static and streaming.
- **Bounded parsing.** Input sizes and entry counts are limited; failures return
  structured errors, never panics on malformed input.
- **No network by default.** Unit tests and core workflows are offline.
- **Structured errors.** Errors carry stable machine-readable codes.

## Heuristics

`inspect --security` runs bounded, deterministic checks from `observatory-audit`:

- specification validation (duplicates, unknown references, arity),
- empty/duplicate names,
- events with more than two prefix topics,
- artifact checks (no code section, no `contractspecv0`, truncated listings,
  oversized specification sections).

### Duplicate entries and metadata consistency

| Id | Severity | Experimental | Rationale |
| -- | -------- | ------------ | --------- |
| `interface.duplicate-function` | ERROR | no | Two functions with one name cannot both be called unambiguously. |
| `interface.duplicate-type` | ERROR | no | A user-defined type name must resolve to a single definition. |
| `interface.duplicate-event` | ERROR | no | Two events with one name cannot be told apart by consumers. |
| `type.duplicate-field` | WARN | yes | Duplicate struct fields are almost certainly a code-generation bug, but the XDR encoding is still decodable. |
| `type.duplicate-case` | WARN | yes | Duplicate union or enum case names break name-based matching; numeric encodings may still be distinct. |
| `artifact.metadata-without-contract-spec` | WARN | yes | Soroban metadata with no `contractspecv0` section means the embedded build metadata does not describe an inspectable interface. Some toolchains may emit metadata before the spec. |
| `artifact.metadata-without-code` | WARN | yes | Metadata claims a Soroban build but the module has no code section, so the artifact cannot be the built contract. |
| `artifact.duplicate-metadata-section` | WARN | yes | More than one `contractmetav0`/`contractenvmetav0` section makes the effective metadata parser-dependent. |

All rules read only the in-memory interface or `WasmReport`, use bounded
iteration over already-parsed collections, and are deterministic. Duplicate
checks report each duplicated name once.

Rules marked `experimental` may change. Heuristic ids are stable for CI.

> These checks are **not** a security audit and provide no guarantees. They catch
> structural inconsistencies, nothing more.

## SSRF policy

The RPC layer ships an address policy (`observatory_rpc::net`) that must be
applied before any live request. It classifies literal addresses and resolves
hostnames, then rejects any non-public result:

- loopback, private (RFC 1918), link-local, and cloud metadata
  (`169.254.169.254`);
- IPv6 unique-local and link-local;
- shared (`100.64.0.0/10`), benchmarking, documentation, unspecified, multicast,
  broadcast, and reserved ranges;
- IPv4-mapped IPv6 addresses are classified as IPv4, so `::ffff:127.0.0.1` is
  rejected.

`SsrfPolicy::strict()` blocks loopback; `SsrfPolicy::local()` permits it for
local development only. Endpoints can be checked with
`Endpoint::validate_ssrf(&policy)`. The live transport calls this before every
request.

The fixture transport performs no network I/O, so SSRF is structurally absent
on that path. The live transport (`observatory-rpc` feature `live`) revalidates
the endpoint and address policy before each connection, does not follow
redirects, does not use environment proxies, and bounds the response body.

## What is out of scope

- Detecting malicious contract logic.
- Validating that a deployed contract is safe.
- Trusting or authenticating RPC endpoints (live request authentication is not
  implemented).

## Reporting

See [SECURITY.md](../SECURITY.md).
