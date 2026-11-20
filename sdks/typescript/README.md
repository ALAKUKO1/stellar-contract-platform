# TypeScript client

Typed ESM client for the Observatory REST API. It uses the platform `fetch`,
`AbortController`, and `node:test`; it has no runtime or development
dependencies and needs Node 22.18 or newer (Node strips the TypeScript types at
runtime).

```ts
import { ObservatoryClient, ApiError } from "@stellar-observatory/client";

const client = new ObservatoryClient({
  baseUrl: "http://127.0.0.1:8080",
  apiKey: "sco_<id>_<secret>",
  timeoutMs: 10_000,
});

try {
  const report = await client.inspect(wasm, false);
  console.log(report.wasm.artifact_sha256);
  console.log(await client.compatibility(wasm, wasm, "strict"));
} catch (error) {
  if (error instanceof ApiError && error.code === "rate_limited") {
    // Back off and retry.
  }
  throw error;
}
```

Methods: `health`, `readiness`, `version`, `inspect`, `spec`, `diff`,
`compatibility`, `fingerprint`, and `security`. Each accepts an optional
`{ signal }` to cancel the request.

- **Authentication**: `apiKey` is sent as `X-API-Key`. The key is never logged.
- **Timeouts**: `timeoutMs` defaults to 30 seconds. A timed-out request rejects
  with `TimeoutError`; a caller abort rejects with `AbortError`.
- **Errors**: `ApiError` carries `status` and a typed `code` (`unauthorized`,
  `forbidden`, `rate_limited`, `wasm`, `rpc`, and so on). Transport, decode, and
  configuration failures use their own classes, all extending
  `ObservatoryError`.

The models in `src/models.ts` mirror the response shapes published in
`docs/schemas/` and the OpenAPI document.

## Tests

```bash
cd sdks/typescript
node --test
```

The tests start a loopback `node:http` mock, so they need no network access and
no real API instance. They cover probes, API-key auth, typed error mapping,
timeouts, caller aborts, and malformed responses.

## Example

```bash
node examples/inspect.ts http://127.0.0.1:8080 sco_... contract.wasm
```
