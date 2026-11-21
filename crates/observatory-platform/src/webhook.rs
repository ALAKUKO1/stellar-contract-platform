//! Webhook subscription registration and management.
//!
//! A subscription selects the events it cares about and carries a random
//! signing secret. Registration validates the endpoint (`https` only, no
//! embedded credentials) and stores exactly one subscription per id. The
//! secret is returned once at creation and is excluded from list/get output.

use std::collections::{BTreeMap, BTreeSet};

use observatory_core::hash::sha256_hex;
use observatory_core::{ObservatoryError, Result};
use serde::{Deserialize, Serialize};

/// A delivery event a subscription may opt into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
    /// A contract analysis completed successfully.
    AnalysisCompleted,
    /// A contract analysis failed.
    AnalysisFailed,
    /// A deployment was verified.
    DeploymentVerified,
}

impl WebhookEvent {
    /// Every supported event, in stable order.
    pub const ALL: &'static [WebhookEvent] = &[
        WebhookEvent::AnalysisCompleted,
        WebhookEvent::AnalysisFailed,
        WebhookEvent::DeploymentVerified,
    ];

    /// The stable wire name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            WebhookEvent::AnalysisCompleted => "analysis_completed",
            WebhookEvent::AnalysisFailed => "analysis_failed",
            WebhookEvent::DeploymentVerified => "deployment_verified",
        }
    }

    /// Parse a stable wire name.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "analysis_completed" => Ok(WebhookEvent::AnalysisCompleted),
            "analysis_failed" => Ok(WebhookEvent::AnalysisFailed),
            "deployment_verified" => Ok(WebhookEvent::DeploymentVerified),
            other => Err(ObservatoryError::invalid(format!(
                "unknown webhook event `{other}`; expected analysis_completed, analysis_failed, or deployment_verified"
            ))),
        }
    }
}

/// A stored webhook subscription. The signing secret is never serialized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebhookSubscription {
    /// Random identifier derived from the secret.
    pub id: String,
    /// The validated HTTPS endpoint.
    pub url: String,
    /// Selected events, sorted and unique.
    pub events: Vec<WebhookEvent>,
    /// Whether deliveries are active.
    pub active: bool,
    /// The signing secret. Excluded from serialization.
    #[serde(skip)]
    pub secret: String,
}

/// The one-time registration response, which reveals the signing secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebhookCreated {
    /// Random identifier derived from the secret.
    pub id: String,
    /// The validated HTTPS endpoint.
    pub url: String,
    /// Selected events, sorted and unique.
    pub events: Vec<WebhookEvent>,
    /// Whether deliveries are active.
    pub active: bool,
    /// The signing secret. Shown once, at creation.
    pub secret: String,
}

/// An in-memory webhook subscription store.
#[derive(Debug, Default)]
pub struct WebhookStore {
    subscriptions: BTreeMap<String, WebhookSubscription>,
}

impl WebhookStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        WebhookStore::default()
    }

    /// Register a subscription and return it, including the signing secret.
    pub fn create(&mut self, url: &str, events: &[WebhookEvent]) -> Result<WebhookCreated> {
        let url = validate_webhook_url(url)?;
        let events = normalize_events(events)?;
        let secret = generate_secret()?;
        let id = sha256_hex(secret.as_bytes())[..16].to_string();
        let subscription = WebhookSubscription {
            id: id.clone(),
            url: url.clone(),
            events: events.clone(),
            active: true,
            secret: secret.clone(),
        };
        self.subscriptions.insert(id.clone(), subscription);
        Ok(WebhookCreated {
            id,
            url,
            events,
            active: true,
            secret,
        })
    }

    /// List every subscription, without secrets.
    #[must_use]
    pub fn list(&self) -> Vec<WebhookSubscription> {
        self.subscriptions.values().cloned().collect()
    }

    /// Get one subscription, without its secret.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<WebhookSubscription> {
        self.subscriptions.get(id).cloned()
    }

    /// Update a subscription. Returns `None` when the id is unknown.
    pub fn update(
        &mut self,
        id: &str,
        url: Option<&str>,
        events: Option<&[WebhookEvent]>,
        active: Option<bool>,
    ) -> Result<Option<WebhookSubscription>> {
        let Some(subscription) = self.subscriptions.get_mut(id) else {
            return Ok(None);
        };
        if let Some(url) = url {
            subscription.url = validate_webhook_url(url)?;
        }
        if let Some(events) = events {
            subscription.events = normalize_events(events)?;
        }
        if let Some(active) = active {
            subscription.active = active;
        }
        Ok(Some(subscription.clone()))
    }

    /// Delete a subscription. Returns whether one was removed.
    pub fn delete(&mut self, id: &str) -> bool {
        self.subscriptions.remove(id).is_some()
    }

    /// The number of stored subscriptions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.subscriptions.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.subscriptions.is_empty()
    }
}

/// Validate and normalize a webhook endpoint URL.
///
/// Only `https` is accepted, the URL must name a host, and embedded credentials
/// (`https://user:pass@host`) are rejected.
pub fn validate_webhook_url(input: &str) -> Result<String> {
    let url = input.trim();
    if url.is_empty() {
        return Err(ObservatoryError::invalid("webhook url is empty"));
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ObservatoryError::invalid(
            "webhook url must not contain whitespace",
        ));
    }
    let rest = match url.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => rest,
        Some((scheme, _)) => {
            return Err(ObservatoryError::invalid(format!(
                "webhook url must use https, found `{scheme}`"
            )))
        }
        None => {
            return Err(ObservatoryError::invalid(
                "webhook url is missing a scheme; expected https://",
            ))
        }
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(ObservatoryError::invalid("webhook url is missing a host"));
    }
    if authority.contains('@') {
        return Err(ObservatoryError::invalid(
            "webhook url must not embed credentials",
        ));
    }
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped
            .split_once(']')
            .map(|(host, _)| host)
            .ok_or_else(|| ObservatoryError::invalid("malformed IPv6 host in webhook url"))?
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    if host.is_empty() {
        return Err(ObservatoryError::invalid("webhook url is missing a host"));
    }
    Ok(url.to_string())
}

fn normalize_events(events: &[WebhookEvent]) -> Result<Vec<WebhookEvent>> {
    if events.is_empty() {
        return Err(ObservatoryError::invalid(
            "a webhook subscription must select at least one event",
        ));
    }
    let unique: BTreeSet<WebhookEvent> = events.iter().copied().collect();
    Ok(unique.into_iter().collect())
}

fn generate_secret() -> Result<String> {
    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret)
        .map_err(|error| ObservatoryError::internal(format!("randomness failure: {error}")))?;
    Ok(hex::encode(secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_urls() {
        assert_eq!(
            validate_webhook_url("https://example.com/hook").unwrap(),
            "https://example.com/hook"
        );
        assert!(validate_webhook_url("https://example.com:8443/hook").is_ok());
        assert!(validate_webhook_url("https://[2001:db8::1]/hook").is_ok());
    }

    #[test]
    fn rejects_insecure_and_malformed_urls() {
        assert!(validate_webhook_url("http://example.com/hook").is_err());
        assert!(validate_webhook_url("example.com/hook").is_err());
        assert!(validate_webhook_url("https://").is_err());
        assert!(validate_webhook_url("https:///path").is_err());
        assert!(validate_webhook_url("https://user:pass@example.com").is_err());
        assert!(validate_webhook_url("https://example.com/has space").is_err());
    }

    #[test]
    fn create_returns_secret_but_list_hides_it() {
        let mut store = WebhookStore::new();
        let created = store
            .create(
                "https://example.com/hook",
                &[
                    WebhookEvent::AnalysisCompleted,
                    WebhookEvent::AnalysisFailed,
                ],
            )
            .unwrap();
        assert_eq!(created.events.len(), 2);
        assert_eq!(created.secret.len(), 64);

        let listed = store.list();
        assert_eq!(listed.len(), 1);
        let value = serde_json::to_value(&listed[0]).unwrap();
        assert!(value.get("secret").is_none());

        let got = store.get(&created.id).unwrap();
        let value = serde_json::to_value(&got).unwrap();
        assert!(value.get("secret").is_none());
    }

    #[test]
    fn events_are_deduplicated_and_required() {
        let mut store = WebhookStore::new();
        let created = store
            .create(
                "https://example.com/hook",
                &[WebhookEvent::AnalysisFailed, WebhookEvent::AnalysisFailed],
            )
            .unwrap();
        assert_eq!(created.events, vec![WebhookEvent::AnalysisFailed]);
        assert!(store.create("https://example.com/hook", &[]).is_err());
    }

    #[test]
    fn update_and_delete_round_trip() {
        let mut store = WebhookStore::new();
        let created = store
            .create(
                "https://example.com/hook",
                &[WebhookEvent::AnalysisCompleted],
            )
            .unwrap();
        let updated = store
            .update(
                &created.id,
                Some("https://example.net/hook"),
                Some(&[WebhookEvent::DeploymentVerified]),
                Some(false),
            )
            .unwrap()
            .unwrap();
        assert_eq!(updated.url, "https://example.net/hook");
        assert_eq!(updated.events, vec![WebhookEvent::DeploymentVerified]);
        assert!(!updated.active);

        assert!(store.delete(&created.id));
        assert!(!store.delete(&created.id));
        assert!(store.is_empty());
        assert!(store
            .update("missing", None, None, Some(true))
            .unwrap()
            .is_none());
    }

    #[test]
    fn update_rejects_invalid_url() {
        let mut store = WebhookStore::new();
        let created = store
            .create(
                "https://example.com/hook",
                &[WebhookEvent::AnalysisCompleted],
            )
            .unwrap();
        assert!(store
            .update(&created.id, Some("http://internal"), None, None)
            .is_err());
    }

    #[test]
    fn event_names_round_trip() {
        for event in WebhookEvent::ALL {
            assert_eq!(WebhookEvent::parse(event.name()).unwrap(), *event);
        }
        assert!(WebhookEvent::parse("nope").is_err());
    }
}
