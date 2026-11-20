/**
 * Typed errors returned by the Observatory client.
 */

/** Stable error codes returned by the API and engine. */
export type ApiErrorCode =
  | "bad_request"
  | "unauthorized"
  | "forbidden"
  | "not_found"
  | "method_not_allowed"
  | "unsupported_media_type"
  | "unprocessable_entity"
  | "rate_limited"
  | "internal"
  | "invalid_input"
  | "decode"
  | "wasm"
  | "spec"
  | "unsupported"
  | "rpc"
  | "verification"
  | "io"
  // Preserve codes added by newer servers without losing the literal typing of
  // the values above.
  | (string & {});

/** Base class for every client failure. */
export class ObservatoryError extends Error {
  constructor(message: string) {
    super(message);
    this.name = new.target.name;
  }
}

/** The API returned a non-success status with a stable error code. */
export class ApiError extends ObservatoryError {
  readonly status: number;
  readonly code: ApiErrorCode;
  readonly rpcKind?: string;
  readonly rpcCode?: number;

  constructor(
    status: number,
    code: ApiErrorCode,
    message: string,
    rpcKind?: string,
    rpcCode?: number,
  ) {
    super(message);
    this.status = status;
    this.code = code;
    this.rpcKind = rpcKind;
    this.rpcCode = rpcCode;
  }
}

/** The request exceeded the configured timeout. */
export class TimeoutError extends ObservatoryError {
  constructor(timeoutMs: number) {
    super(`request timed out after ${timeoutMs}ms`);
  }
}

/** The caller aborted the request. */
export class AbortError extends ObservatoryError {
  constructor() {
    super("request was aborted");
  }
}

/** The transport failed before a complete response was received. */
export class TransportError extends ObservatoryError {}

/** The response was not the expected JSON shape. */
export class DecodeError extends ObservatoryError {}

/** The client configuration is invalid. */
export class ConfigError extends ObservatoryError {}
