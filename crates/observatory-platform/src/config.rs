//! Platform configuration with strict validation.

use observatory_core::{ObservatoryError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ratelimit::RateLimitConfig;

/// Configuration for the platform/API layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformConfig {
    /// Whether analysis endpoints require authentication.
    #[serde(default = "default_true")]
    pub require_auth: bool,
    /// Rate limiting configuration.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    /// Cross-origin resource sharing policy. Denies all origins by default.
    #[serde(default)]
    pub cors: CorsConfig,
}

fn default_true() -> bool {
    true
}

impl Default for PlatformConfig {
    fn default() -> Self {
        PlatformConfig {
            require_auth: true,
            rate_limit: RateLimitConfig::default(),
            cors: CorsConfig::default(),
        }
    }
}

/// A secure-by-default CORS policy.
///
/// An empty `allowed_origins` list denies every cross-origin request. Credentials
/// are never enabled, so a wildcard origin can never be combined with cookies or
/// authorization state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CorsConfig {
    /// Exact origins that may make cross-origin requests.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Methods advertised to allowed origins during preflight.
    #[serde(default)]
    pub allowed_methods: Vec<String>,
    /// Request headers advertised to allowed origins during preflight.
    #[serde(default)]
    pub allowed_headers: Vec<String>,
}

impl CorsConfig {
    /// Whether `origin` may make cross-origin requests.
    #[must_use]
    pub fn is_allowed_origin(&self, origin: &str) -> bool {
        self.allowed_origins
            .iter()
            .any(|allowed| allowed == "*" || allowed.eq_ignore_ascii_case(origin))
    }

    /// Whether a wildcard origin is configured.
    #[must_use]
    pub fn allows_any_origin(&self) -> bool {
        self.allowed_origins.iter().any(|origin| origin == "*")
    }

    /// Whether credentialed requests are enabled.
    ///
    /// This implementation never advertises `Access-Control-Allow-Credentials`,
    /// which keeps wildcard origins safe.
    #[must_use]
    pub fn credentials_allowed(&self) -> bool {
        false
    }

    /// The methods advertised during preflight.
    #[must_use]
    pub fn methods(&self) -> Vec<String> {
        if self.allowed_methods.is_empty() {
            ["GET", "POST", "PATCH", "DELETE", "OPTIONS"]
                .iter()
                .map(|method| (*method).to_string())
                .collect()
        } else {
            self.allowed_methods.clone()
        }
    }

    /// The headers advertised during preflight.
    #[must_use]
    pub fn headers(&self) -> Vec<String> {
        if self.allowed_headers.is_empty() {
            ["content-type", "authorization", "x-api-key"]
                .iter()
                .map(|header| (*header).to_string())
                .collect()
        } else {
            self.allowed_headers.clone()
        }
    }
}

impl PlatformConfig {
    /// Parse configuration from a JSON value, rejecting unknown fields.
    pub fn from_json(value: &Value) -> Result<Self> {
        serde_json::from_value(value.clone()).map_err(|error| {
            ObservatoryError::invalid(format!("invalid platform configuration: {error}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_sane() {
        let config = PlatformConfig::default();
        assert!(config.require_auth);
        assert_eq!(config.rate_limit.limit, 120);
    }

    #[test]
    fn parses_valid_configuration() {
        let config = PlatformConfig::from_json(&json!({
            "require_auth": false,
            "rate_limit": { "limit": 10, "window_secs": 30 }
        }))
        .unwrap();
        assert!(!config.require_auth);
        assert_eq!(config.rate_limit.limit, 10);
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = PlatformConfig::from_json(&json!({ "unknown": true })).unwrap_err();
        assert!(matches!(error, ObservatoryError::InvalidInput(_)));
    }

    #[test]
    fn cors_denies_by_default() {
        let config = PlatformConfig::default();
        assert!(!config.cors.is_allowed_origin("https://app.example.com"));
        assert!(!config.cors.allows_any_origin());
        assert!(!config.cors.credentials_allowed());
    }

    #[test]
    fn parses_cors_allow_list() {
        let config = PlatformConfig::from_json(&json!({
            "cors": {
                "allowed_origins": ["https://app.example.com"],
                "allowed_methods": ["GET", "POST"],
                "allowed_headers": ["content-type"]
            }
        }))
        .unwrap();
        assert!(config.cors.is_allowed_origin("https://app.example.com"));
        assert!(!config.cors.is_allowed_origin("https://evil.example.com"));
        assert_eq!(config.cors.methods(), vec!["GET", "POST"]);
        assert_eq!(config.cors.headers(), vec!["content-type"]);
    }

    #[test]
    fn wildcard_origin_never_enables_credentials() {
        let config = PlatformConfig::from_json(&json!({
            "cors": { "allowed_origins": ["*"] }
        }))
        .unwrap();
        assert!(config.cors.allows_any_origin());
        assert!(!config.cors.credentials_allowed());
    }
}
