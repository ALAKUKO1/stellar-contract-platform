//! Live HTTPS JSON-RPC transport.
//!
//! Enabled with the `live` feature, which is off by default so the core stays
//! offline. The transport re-runs endpoint validation and the SSRF policy before
//! every request, applies a global timeout, and bounds the response body.

use std::time::Duration;

use observatory_core::{ObservatoryError, Result};
use serde_json::Value;

use crate::{Endpoint, SsrfPolicy, Transport};

/// Default per-request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default maximum response body size.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// A blocking HTTPS transport for the Stellar RPC JSON-RPC API.
///
/// Loopback endpoints use [`SsrfPolicy::local`] so local development and mock
/// servers work; every other endpoint uses [`SsrfPolicy::strict`]. Redirects are
/// never followed.
pub struct HttpTransport {
    endpoint: Endpoint,
    ssrf: SsrfPolicy,
    agent: ureq::Agent,
    timeout: Duration,
    max_response_bytes: usize,
}

impl HttpTransport {
    /// Create a transport with the default timeout and response limit.
    pub fn new(endpoint: Endpoint) -> Result<Self> {
        Self::with_limits(endpoint, DEFAULT_TIMEOUT, DEFAULT_MAX_RESPONSE_BYTES)
    }

    /// Create a transport with explicit limits.
    pub fn with_limits(
        endpoint: Endpoint,
        timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self> {
        if timeout.is_zero() {
            return Err(ObservatoryError::invalid(
                "RPC request timeout must be greater than zero",
            ));
        }
        if max_response_bytes == 0 {
            return Err(ObservatoryError::invalid(
                "RPC maximum response size must be greater than zero",
            ));
        }
        // Re-validate the URL itself before building any agent state.
        let endpoint = Endpoint::parse(endpoint.as_str())?;
        let ssrf = if endpoint.is_loopback() {
            SsrfPolicy::local()
        } else {
            SsrfPolicy::strict()
        };
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(true)
            .proxy(None)
            .build();
        Ok(HttpTransport {
            endpoint,
            ssrf,
            agent: ureq::Agent::new_with_config(config),
            timeout,
            max_response_bytes,
        })
    }

    /// The validated endpoint in use.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl Transport for HttpTransport {
    fn send(&self, request: &Value) -> Result<Value> {
        let endpoint = Endpoint::parse(self.endpoint.as_str())?;
        endpoint.validate_ssrf(&self.ssrf)?;

        let body = serde_json::to_vec(request).map_err(|error| {
            ObservatoryError::rpc(format!("cannot encode RPC request: {error}"))
        })?;
        let response = self
            .agent
            .post(endpoint.as_str())
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .send(body)
            .map_err(|error| transport_error(&endpoint, self.timeout, &error))?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(ObservatoryError::rpc(format!(
                "RPC endpoint {} returned HTTP {status}",
                endpoint.host()
            )));
        }

        let mut response = response;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(self.max_response_bytes as u64)
            .read_to_vec()
            .map_err(|error| response_error(&endpoint, self.max_response_bytes, &error))?;
        serde_json::from_slice(&bytes).map_err(|error| {
            ObservatoryError::rpc(format!(
                "RPC endpoint {} returned invalid JSON: {error}",
                endpoint.host()
            ))
        })
    }
}

fn transport_error(
    endpoint: &Endpoint,
    timeout: Duration,
    error: &ureq::Error,
) -> ObservatoryError {
    let detail = match error {
        ureq::Error::Timeout(_) => {
            format!("request timed out after {}s", timeout.as_secs())
        }
        ureq::Error::HostNotFound => "host not found".to_string(),
        ureq::Error::ConnectionFailed => "connection failed".to_string(),
        ureq::Error::Io(error) => format!("I/O error ({:?})", error.kind()),
        ureq::Error::Tls(detail) => format!("TLS error: {detail}"),
        ureq::Error::StatusCode(status) => format!("unexpected HTTP status {status}"),
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => {
            "redirects are not followed".to_string()
        }
        ureq::Error::BodyExceedsLimit(limit) => {
            format!("response body exceeded the {limit} byte limit")
        }
        ureq::Error::BadUri(_) => "malformed endpoint URL".to_string(),
        ureq::Error::Protocol(_) => "HTTP protocol error".to_string(),
        _ => "transport failure".to_string(),
    };
    ObservatoryError::rpc(format!(
        "RPC transport to {} failed: {detail}",
        endpoint.host()
    ))
}

fn response_error(
    endpoint: &Endpoint,
    max_response_bytes: usize,
    error: &ureq::Error,
) -> ObservatoryError {
    match error {
        ureq::Error::BodyExceedsLimit(_) => ObservatoryError::rpc(format!(
            "RPC endpoint {} response exceeded the {max_response_bytes} byte limit",
            endpoint.host()
        )),
        other => transport_error(endpoint, Duration::ZERO, other),
    }
}
