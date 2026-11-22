//! Machine-readable error catalog.
//!
//! Every code emitted by the API appears exactly once here with its HTTP
//! status, description, and retryability. [`ApiError`](crate::ApiError) builds
//! its status from this table, so a response cannot use a code with a different
//! status than the catalog publishes.

use serde::Serialize;

/// One published error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ErrorCatalogEntry {
    /// Stable machine-readable code emitted in the error envelope.
    pub code: &'static str,
    /// HTTP status returned with the code.
    pub status: u16,
    /// What the code means.
    pub description: &'static str,
    /// Whether retrying the same request may succeed.
    pub retryable: bool,
}

/// The error catalog. Core taxonomy codes come first, then API-level codes.
pub const ERROR_CATALOG: &[ErrorCatalogEntry] = &[
    ErrorCatalogEntry {
        code: "invalid_input",
        status: 400,
        description: "The request or a supplied input is invalid.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "io",
        status: 500,
        description: "A filesystem or I/O operation failed.",
        retryable: true,
    },
    ErrorCatalogEntry {
        code: "decode",
        status: 400,
        description: "The input could not be decoded or parsed.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "unsupported",
        status: 422,
        description: "The input uses a feature this build does not support.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "wasm",
        status: 400,
        description: "The WASM artifact is malformed.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "spec",
        status: 400,
        description: "The contract specification is malformed.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "rpc",
        status: 502,
        description: "The upstream RPC endpoint failed.",
        retryable: true,
    },
    ErrorCatalogEntry {
        code: "verification",
        status: 422,
        description: "Verification failed or was inconclusive.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "internal",
        status: 500,
        description: "An internal invariant was violated.",
        retryable: true,
    },
    ErrorCatalogEntry {
        code: "unauthorized",
        status: 401,
        description: "A valid API key is required.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "forbidden",
        status: 403,
        description: "The API key lacks the required permission.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "bad_request",
        status: 400,
        description: "The request was malformed.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "not_found",
        status: 404,
        description: "The requested resource or route does not exist.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "method_not_allowed",
        status: 405,
        description: "The route exists but does not support this method.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "unsupported_media_type",
        status: 415,
        description: "The request body must be application/json.",
        retryable: false,
    },
    ErrorCatalogEntry {
        code: "rate_limited",
        status: 429,
        description: "The caller exceeded its rate limit; retry after the reset window.",
        retryable: true,
    },
    ErrorCatalogEntry {
        code: "cors_origin_denied",
        status: 403,
        description: "The origin is not allowed by the CORS policy.",
        retryable: false,
    },
];

/// Look up a code in the catalog.
#[must_use]
pub fn entry(code: &str) -> Option<&'static ErrorCatalogEntry> {
    ERROR_CATALOG.iter().find(|entry| entry.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use observatory_core::ErrorCode;

    #[test]
    fn catalog_covers_every_core_error_code() {
        for code in ErrorCode::ALL {
            let entry = entry(code.as_str())
                .unwrap_or_else(|| panic!("{} missing from error catalog", code.as_str()));
            assert_eq!(entry.code, code.as_str());
        }
    }

    #[test]
    fn catalog_codes_are_unique_and_statuses_are_valid() {
        let mut seen = std::collections::BTreeSet::new();
        for entry in ERROR_CATALOG {
            assert!(seen.insert(entry.code), "duplicate code {}", entry.code);
            assert!(
                (400..600).contains(&entry.status),
                "invalid status {} for {}",
                entry.status,
                entry.code
            );
            assert!(
                !entry.description.is_empty(),
                "{} has no description",
                entry.code
            );
        }
        assert!(entry("no_such_code").is_none());
    }
}
