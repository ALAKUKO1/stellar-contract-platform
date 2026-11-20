/**
 * Typed ESM client for the Stellar Contract Observatory REST API.
 *
 * Uses the platform `fetch` and `AbortController`, so it has no runtime
 * dependencies. Requests are bounded by a configurable timeout and callers can
 * pass their own `AbortSignal`. Redirects are followed by `fetch`; point the
 * client at a trusted Observatory instance.
 */

import {
  AbortError,
  ApiError,
  ConfigError,
  DecodeError,
  ObservatoryError,
  TimeoutError,
  TransportError,
  type ApiErrorCode,
} from "./errors.ts";
import type {
  AuditReport,
  CompatibilityAssessment,
  ContractSpec,
  ErrorReport,
  FingerprintResponse,
  Health,
  InspectResponse,
  InterfaceDiff,
  Readiness,
  Report,
  SpecResponse,
  SpecSummary,
  VersionInfo,
} from "./models.ts";

/** Default per-request timeout in milliseconds. */
export const DEFAULT_TIMEOUT_MS = 30_000;

/** Compatibility policy accepted by the compatibility endpoint. */
export type Policy = "strict" | "lenient";

export interface ClientOptions {
  /** Base URL of the API, for example `http://127.0.0.1:8080`. */
  baseUrl: string;
  /** API key sent as `X-API-Key`. */
  apiKey?: string;
  /** Per-request timeout in milliseconds; defaults to 30 seconds. */
  timeoutMs?: number;
  /** Override `fetch` (useful for tests and non-standard runtimes). */
  fetch?: typeof fetch;
}

export interface RequestOptions {
  /** Cancel the request. Aborting rejects with {@link AbortError}. */
  signal?: AbortSignal;
}

export class ObservatoryClient {
  readonly #baseUrl: string;
  readonly #apiKey?: string;
  readonly #timeoutMs: number;
  readonly #fetch: typeof fetch;

  constructor(options: ClientOptions) {
    const baseUrl = options.baseUrl.replace(/\/+$/, "");
    if (!/^https?:\/\//.test(baseUrl)) {
      throw new ConfigError("baseUrl must start with http:// or https://");
    }
    const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
      throw new ConfigError("timeoutMs must be greater than zero");
    }
    this.#baseUrl = baseUrl;
    this.#apiKey = options.apiKey;
    this.#timeoutMs = timeoutMs;
    this.#fetch = options.fetch ?? fetch;
  }

  /** The configured base URL, without a trailing slash. */
  get baseUrl(): string {
    return this.#baseUrl;
  }

  health(options?: RequestOptions): Promise<Health> {
    return this.#request("health", "GET", "/api/v1/health", undefined, options);
  }

  readiness(options?: RequestOptions): Promise<Readiness> {
    return this.#request(
      "readiness",
      "GET",
      "/api/v1/readiness",
      undefined,
      options,
    );
  }

  version(options?: RequestOptions): Promise<VersionInfo> {
    return this.#request("version", "GET", "/api/v1/version", undefined, options);
  }

  inspect(
    wasm: Uint8Array,
    security = false,
    options?: RequestOptions,
  ): Promise<InspectResponse> {
    return this.#request(
      "inspect",
      "POST",
      "/api/v1/contracts/inspect",
      { wasm_base64: toBase64(wasm), security },
      options,
    );
  }

  spec(wasm: Uint8Array, options?: RequestOptions): Promise<SpecResponse> {
    return this.#request(
      "spec",
      "POST",
      "/api/v1/contracts/spec",
      { wasm_base64: toBase64(wasm) },
      options,
    );
  }

  diff(
    oldWasm: Uint8Array,
    newWasm: Uint8Array,
    options?: RequestOptions,
  ): Promise<InterfaceDiff> {
    return this.#request(
      "diff",
      "POST",
      "/api/v1/contracts/diff",
      {
        old_wasm_base64: toBase64(oldWasm),
        new_wasm_base64: toBase64(newWasm),
      },
      options,
    );
  }

  compatibility(
    oldWasm: Uint8Array,
    newWasm: Uint8Array,
    policy: Policy = "strict",
    options?: RequestOptions,
  ): Promise<CompatibilityAssessment> {
    return this.#request(
      "compat",
      "POST",
      "/api/v1/contracts/compatibility",
      {
        old_wasm_base64: toBase64(oldWasm),
        new_wasm_base64: toBase64(newWasm),
        policy,
      },
      options,
    );
  }

  fingerprint(
    wasm: Uint8Array,
    options?: RequestOptions,
  ): Promise<FingerprintResponse> {
    return this.#request(
      "fingerprint",
      "POST",
      "/api/v1/contracts/fingerprint",
      { wasm_base64: toBase64(wasm) },
      options,
    );
  }

  security(wasm: Uint8Array, options?: RequestOptions): Promise<AuditReport> {
    return this.#request(
      "security",
      "POST",
      "/api/v1/contracts/security",
      { wasm_base64: toBase64(wasm) },
      options,
    );
  }

  async #request<T>(
    kind: string,
    method: "GET" | "POST",
    path: string,
    body: unknown,
    options?: RequestOptions,
  ): Promise<T> {
    if (options?.signal?.aborted) {
      throw new AbortError();
    }

    const controller = new AbortController();
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, this.#timeoutMs);
    const forwardAbort = () => controller.abort();
    options?.signal?.addEventListener("abort", forwardAbort, { once: true });

    const headers: Record<string, string> = { accept: "application/json" };
    if (this.#apiKey !== undefined) {
      headers["x-api-key"] = this.#apiKey;
    }
    if (body !== undefined) {
      headers["content-type"] = "application/json";
    }

    let response: Response;
    try {
      response = await this.#fetch(`${this.#baseUrl}${path}`, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: controller.signal,
      });
    } catch (error) {
      if (options?.signal?.aborted) {
        throw new AbortError();
      }
      if (timedOut) {
        throw new TimeoutError(this.#timeoutMs);
      }
      throw new TransportError(
        error instanceof Error ? error.message : String(error),
      );
    } finally {
      clearTimeout(timer);
      options?.signal?.removeEventListener("abort", forwardAbort);
    }

    const text = await response.text();
    if (!response.ok) {
      throw apiError(response.status, text);
    }

    let envelope: Report<T>;
    try {
      envelope = JSON.parse(text) as Report<T>;
    } catch (error) {
      throw new DecodeError(
        `invalid JSON response: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    if (envelope === null || typeof envelope !== "object") {
      throw new DecodeError("response was not a report envelope");
    }
    if (envelope.kind !== kind) {
      throw new DecodeError(`expected kind \`${kind}\`, got \`${envelope.kind}\``);
    }
    return envelope.data;
  }
}

function apiError(status: number, text: string): ObservatoryError {
  let body: Partial<ErrorReport> = {};
  try {
    body = JSON.parse(text) as Partial<ErrorReport>;
  } catch {
    return new ApiError(status, "unknown", text);
  }
  if (typeof body.code !== "string" || typeof body.message !== "string") {
    return new ApiError(status, "unknown", text);
  }
  return new ApiError(
    status,
    body.code as ApiErrorCode,
    body.message,
    body.rpc_kind,
    body.rpc_code,
  );
}

function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

export type {
  AuditReport,
  CompatibilityAssessment,
  ContractSpec,
  FingerprintResponse,
  Health,
  InspectResponse,
  InterfaceDiff,
  Readiness,
  SpecResponse,
  SpecSummary,
  VersionInfo,
};
export type { ApiErrorCode };
export {
  AbortError,
  ApiError,
  ConfigError,
  DecodeError,
  ObservatoryError,
  TimeoutError,
  TransportError,
};
