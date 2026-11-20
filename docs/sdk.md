# SDKs

Typed clients for the Observatory REST API. Clients mirror the published
OpenAPI document, authenticate with an API key, and preserve the API's stable
error codes.

## Rust client

`observatory-client` is a blocking client that reuses the engine's report types,
so responses are strongly typed without depending on the server crate.

```toml
[dependencies]
observatory-client = { path = "crates/observatory-client" }
```

```rust
use std::time::Duration;
use observatory_client::{Client, ClientError, ApiErrorCode, Policy};

fn main() -> Result<(), ClientError> {
    let client = Client::builder("http://127.0.0.1:8080")
        .api_key("sco_<id>_<secret>")
        .timeout(Duration::from_secs(10))
        .build()?;

    let wasm = std::fs::read("contract.wasm").expect("read artifact");
    let report = client.inspect(&wasm, false)?;
    println!("{}", report.wasm.artifact_sha256);

    let assessment = client.compatibility(&wasm, &wasm, Policy::Strict)?;
    println!("{:?}", assessment.status);
    Ok(())
}
```

Methods: `health`, `readiness`, `version`, `inspect`, `spec`, `diff`,
`compatibility`, `fingerprint`, and `security`.

- **Authentication**: `.api_key(key)` sends `X-API-Key`. The key is never
  logged.
- **Timeouts**: `.timeout(Duration)` defaults to 30 seconds; a zero timeout is
  rejected at build time.
- **Bounds**: artifacts larger than the engine limit are rejected before
  encoding, and response bodies are capped at 8 MiB by default
  (`.max_response_bytes`). Redirects are not followed.
- **Errors**: `ClientError::Api` carries the HTTP status plus a typed
  `ApiErrorCode` (`Unauthorized`, `Forbidden`, `RateLimited`, `Wasm`, `Rpc`,
  and so on, with `Other` preserving unknown codes). Transport failures,
  timeouts, oversized responses, and decode failures are distinct variants.

See `crates/observatory-client/examples/inspect.rs` for a runnable example.

```bash
cargo run -p observatory-client --example inspect -- \
  http://127.0.0.1:8080 sco_... contract.wasm
```

The integration tests start a real API instance on an ephemeral loopback port
and round-trip every endpoint; no public network access is required.

## TypeScript client

`sdks/typescript` is a dependency-free ESM client for Node 22.18 or newer (Node
strips the TypeScript types at runtime, so no build step is needed).

```ts
import { ObservatoryClient, ApiError } from "@stellar-observatory/client";

const client = new ObservatoryClient({
  baseUrl: "http://127.0.0.1:8080",
  apiKey: "sco_<id>_<secret>",
  timeoutMs: 10_000,
});

const report = await client.inspect(wasm, false);
console.log(report.wasm.artifact_sha256);
```

It exposes the same methods as the Rust client. Timeouts reject with
`TimeoutError`, caller aborts with `AbortError`, and API failures with a typed
`ApiError` carrying the stable `code`. Models mirror `docs/schemas/` and the
OpenAPI document. Run the tests with `node --test` from `sdks/typescript`; see
`sdks/typescript/README.md`.
