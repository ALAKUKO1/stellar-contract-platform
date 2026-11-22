//! API-key authentication.
//!
//! Keys are generated with OS randomness and stored only as SHA-256 hashes. The
//! plaintext key is returned exactly once, at creation. The record never
//! serializes the hash, so a leaked record cannot be replayed.

use std::collections::BTreeMap;

use observatory_core::error::redact_secrets;
use observatory_core::hash::sha256_hex;
use observatory_core::{ObservatoryError, Result};
use serde::Serialize;

use crate::rbac::{Permission, Role};

/// Prefix identifying an Observatory API key.
pub const KEY_PREFIX: &str = "sco";

/// A stored API-key record. The secret hash is never serialized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiKeyRecord {
    /// Public key identifier (also present in the plaintext key).
    pub id: String,
    /// SHA-256 of the secret. Excluded from serialization.
    #[serde(skip)]
    pub hash: String,
    /// The role granted to the key.
    pub role: Role,
    /// Optional subset of the role's permissions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<Permission>>,
    /// A human label.
    pub label: String,
    /// Whether the key has been revoked.
    pub revoked: bool,
}

/// The authenticated identity behind a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Principal {
    /// The API-key id.
    pub key_id: String,
    /// The granted role.
    pub role: Role,
    /// Optional subset of the role's permissions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<Permission>>,
}

/// A pluggable API-key store.
///
/// Implementations must never store or log the plaintext key. The in-memory
/// backend is the default for tests; [`crate::store::FileKeyStore`] persists
/// only metadata and hashes so keys survive a restart.
pub trait KeyStore: Send + Sync {
    /// Generate a new key, returning the plaintext exactly once.
    fn generate(
        &mut self,
        role: Role,
        label: &str,
        scopes: Option<Vec<Permission>>,
    ) -> Result<(String, ApiKeyRecord)>;
    /// Insert a record from a pre-computed hash (fixtures/imports).
    fn insert_hashed(&mut self, id: &str, hash: &str, role: Role, label: &str) -> ApiKeyRecord;
    /// Register a key from its plaintext form.
    fn insert_plaintext(
        &mut self,
        plaintext: &str,
        role: Role,
        label: &str,
    ) -> Result<ApiKeyRecord>;
    /// Revoke a key by id.
    fn revoke(&mut self, id: &str) -> bool;
    /// Change a live key's role.
    fn set_role(&mut self, id: &str, role: Role) -> bool;
    /// Authenticate a presented key.
    fn authenticate(&self, provided: &str) -> Option<Principal>;
    /// List key metadata, sorted by id. Records never serialize the hash.
    fn list(&self) -> Vec<ApiKeyRecord>;
    /// Number of stored keys.
    fn len(&self) -> usize;
    /// Whether the store is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// An in-memory API-key store.
#[derive(Debug, Default)]
pub struct ApiKeyStore {
    keys: BTreeMap<String, ApiKeyRecord>,
}

impl ApiKeyStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        ApiKeyStore::default()
    }

    /// Generate a new key for `role`. Returns `(plaintext, record)`.
    ///
    /// The plaintext is shown once and never stored. `scopes` narrows the role
    /// when present; callers are expected to validate it against the role first.
    pub fn generate(
        &mut self,
        role: Role,
        label: &str,
        scopes: Option<Vec<Permission>>,
    ) -> Result<(String, ApiKeyRecord)> {
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret)
            .map_err(|error| ObservatoryError::internal(format!("randomness failure: {error}")))?;
        let secret_hex = hex::encode(secret);
        let id = sha256_hex(format!("{label}:{secret_hex}").as_bytes())[..16].to_string();
        let plaintext = format!("{KEY_PREFIX}_{id}_{secret_hex}");
        let record = ApiKeyRecord {
            id: id.clone(),
            hash: sha256_hex(secret_hex.as_bytes()),
            role,
            scopes,
            label: label.to_string(),
            revoked: false,
        };
        self.keys.insert(id, record.clone());
        Ok((plaintext, record))
    }

    /// Insert a record directly (for fixtures and imports).
    pub fn insert_hashed(
        &mut self,
        id: impl Into<String>,
        hash: impl Into<String>,
        role: Role,
        label: &str,
    ) -> ApiKeyRecord {
        let record = ApiKeyRecord {
            id: id.into(),
            hash: hash.into(),
            role,
            scopes: None,
            label: label.to_string(),
            revoked: false,
        };
        self.keys.insert(record.id.clone(), record.clone());
        record
    }

    /// Insert a complete record, including its revocation state.
    ///
    /// Used when loading a persistent backend. The hash is expected to be an
    /// already-computed SHA-256 digest.
    pub fn insert_record(&mut self, record: ApiKeyRecord) -> ApiKeyRecord {
        self.keys.insert(record.id.clone(), record.clone());
        record
    }

    /// Register a key from its plaintext form (`sco_<id>_<secret>`).
    ///
    /// Only the hash is stored. This is how a server is configured with keys
    /// without a persistence layer.
    pub fn insert_plaintext(
        &mut self,
        plaintext: &str,
        role: Role,
        label: &str,
    ) -> Result<ApiKeyRecord> {
        let redacted = redact_secrets(plaintext.trim());
        let mut parts = plaintext.trim().splitn(3, '_');
        if parts.next() != Some(KEY_PREFIX) {
            return Err(ObservatoryError::invalid(format!(
                "API key `{redacted}` must start with `sco_`"
            )));
        }
        let id = parts
            .next()
            .ok_or_else(|| ObservatoryError::invalid("API key is missing its id"))?
            .to_string();
        let secret = parts
            .next()
            .ok_or_else(|| ObservatoryError::invalid("API key is missing its secret"))?;
        let record = ApiKeyRecord {
            id: id.clone(),
            hash: sha256_hex(secret.as_bytes()),
            role,
            scopes: None,
            label: label.to_string(),
            revoked: false,
        };
        self.keys.insert(id, record.clone());
        Ok(record)
    }

    /// Change a live key's role. Returns whether the key was updated.
    pub fn set_role(&mut self, id: &str, role: Role) -> bool {
        match self.keys.get_mut(id) {
            Some(record) if !record.revoked => {
                record.role = role;
                true
            }
            _ => false,
        }
    }

    /// Revoke a key by id. Returns whether a key was revoked.
    pub fn revoke(&mut self, id: &str) -> bool {
        match self.keys.get_mut(id) {
            Some(record) => {
                record.revoked = true;
                true
            }
            None => false,
        }
    }

    /// Key metadata, sorted by id.
    ///
    /// The returned records carry the hash in memory, but [`ApiKeyRecord`]
    /// skips it during serialization.
    #[must_use]
    pub fn list(&self) -> Vec<ApiKeyRecord> {
        self.keys.values().cloned().collect()
    }

    /// The number of stored keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Authenticate a presented key, returning the principal on success.
    #[must_use]
    pub fn authenticate(&self, provided: &str) -> Option<Principal> {
        let mut parts = provided.trim().splitn(3, '_');
        if parts.next()? != KEY_PREFIX {
            return None;
        }
        let id = parts.next()?;
        let secret = parts.next()?;
        let record = self.keys.get(id)?;
        if record.revoked {
            return None;
        }
        if sha256_hex(secret.as_bytes()) != record.hash {
            return None;
        }
        Some(Principal {
            key_id: record.id.clone(),
            role: record.role,
            scopes: record.scopes.clone(),
        })
    }
}

impl KeyStore for ApiKeyStore {
    fn generate(
        &mut self,
        role: Role,
        label: &str,
        scopes: Option<Vec<Permission>>,
    ) -> Result<(String, ApiKeyRecord)> {
        ApiKeyStore::generate(self, role, label, scopes)
    }

    fn insert_hashed(&mut self, id: &str, hash: &str, role: Role, label: &str) -> ApiKeyRecord {
        ApiKeyStore::insert_hashed(self, id, hash, role, label)
    }

    fn insert_plaintext(
        &mut self,
        plaintext: &str,
        role: Role,
        label: &str,
    ) -> Result<ApiKeyRecord> {
        ApiKeyStore::insert_plaintext(self, plaintext, role, label)
    }

    fn revoke(&mut self, id: &str) -> bool {
        ApiKeyStore::revoke(self, id)
    }

    fn set_role(&mut self, id: &str, role: Role) -> bool {
        ApiKeyStore::set_role(self, id, role)
    }

    fn authenticate(&self, provided: &str) -> Option<Principal> {
        ApiKeyStore::authenticate(self, provided)
    }

    fn list(&self) -> Vec<ApiKeyRecord> {
        ApiKeyStore::list(self)
    }

    fn len(&self) -> usize {
        ApiKeyStore::len(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_authenticate() {
        let mut store = ApiKeyStore::new();
        let (plaintext, record) = store.generate(Role::Developer, "ci", None).unwrap();
        assert!(plaintext.starts_with("sco_"));
        let principal = store.authenticate(&plaintext).unwrap();
        assert_eq!(principal.key_id, record.id);
        assert_eq!(principal.role, Role::Developer);
    }

    #[test]
    fn wrong_secret_and_prefix_fail() {
        let mut store = ApiKeyStore::new();
        let (_, record) = store.generate(Role::Viewer, "x", None).unwrap();
        assert!(store.authenticate("sco_wrong_deadbeef").is_none());
        assert!(store.authenticate("nope").is_none());
        let forged = format!("sco_{}_00", record.id);
        assert!(store.authenticate(&forged).is_none());
    }

    #[test]
    fn revoked_keys_fail() {
        let mut store = ApiKeyStore::new();
        let (plaintext, record) = store.generate(Role::Admin, "ops", None).unwrap();
        assert!(store.revoke(&record.id));
        assert!(store.authenticate(&plaintext).is_none());
        assert!(!store.revoke("missing"));
    }

    #[test]
    fn records_never_serialize_the_hash() {
        let mut store = ApiKeyStore::new();
        let (_, record) = store.generate(Role::Viewer, "x", None).unwrap();
        let json = serde_json::to_string(&record).unwrap();
        assert!(!json.contains(&record.hash));
        assert!(!json.contains("hash"));
    }

    #[test]
    fn auth_errors_and_logs_never_echo_the_presented_key() {
        let presented = "https://user:pass@example.org/rpc?api_key=SECRET";
        let mut store = ApiKeyStore::new();
        let rendered = store
            .insert_plaintext(presented, Role::Viewer, "bad")
            .unwrap_err()
            .to_string();
        for secret in ["user", "pass", "SECRET", presented] {
            assert!(!rendered.contains(secret), "{rendered}");
        }

        let (plaintext, record) = store.generate(Role::Viewer, "ci", None).unwrap();
        assert!(!format!("{record:?}").contains(&plaintext));
        assert!(!format!("{:?}", store.authenticate(&plaintext)).contains(&plaintext));
    }

    #[test]
    fn scopes_survive_authentication() {
        let mut store = ApiKeyStore::new();
        let (plaintext, record) = store
            .generate(Role::Developer, "ci", Some(vec![Permission::RunAnalysis]))
            .unwrap();
        let principal = store.authenticate(&plaintext).unwrap();
        assert_eq!(principal.role, record.role);
        assert_eq!(principal.scopes, Some(vec![Permission::RunAnalysis]));
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("run_analysis"));
    }
}
