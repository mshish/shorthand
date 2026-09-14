//! Fork-only: provider secrets live in the OS credential store, never in
//! settings_store.json or a plugin's data.json. Design:
//! PLANS/SHORTHAND_APP_CREDENTIALS_IMPLEMENTATION_PLAN.md (Buzz workspace).
//!
//! A secret never leaves this module except through `get`. `CredentialError`
//! carries the platform's own message and `status` answers with an enum, so
//! nothing here can put a secret in a log line, an error, or a settings
//! payload. When secure storage cannot be reached the answer is
//! `Unavailable` — there is deliberately no plaintext fallback.

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use url::Url;

/// The `user` half of every keyring entry; the service string carries the
/// slot. Windows entries therefore read `Shorthand/shorthand@<service>`.
const KEYRING_USER: &str = "shorthand";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LlmProvider {
    Openai,
    Anthropic,
    Ollama,
    OpenaiCompatible,
}

impl LlmProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Ollama => "ollama",
            Self::OpenaiCompatible => "openai-compatible",
        }
    }

    /// Local endpoints legitimately authenticate nothing; hosted providers
    /// never do.
    pub fn allows_no_secret(self) -> bool {
        matches!(self, Self::Ollama | Self::OpenaiCompatible)
    }
}

/// Which secret is being asked about. `NotesLlm` and `NotesAcp` are the wire
/// shapes the request socket accepts; `PostProcess` is the app's own slot and
/// never arrives from a client (Task A4 rejects it there, because
/// `#[serde(skip)]` cannot be applied to a variant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CredentialSlot {
    PostProcess {
        provider_id: String,
    },
    NotesLlm {
        provider: LlmProvider,
        #[serde(deserialize_with = "origin_only")]
        origin: String,
    },
    NotesAcp {
        #[serde(rename = "vaultId")]
        vault_id: String,
        #[serde(deserialize_with = "origin_only")]
        origin: String,
    },
}

impl CredentialSlot {
    /// The keyring service string. These are storage keys: changing one
    /// orphans every secret already saved under the old spelling, so they are
    /// pinned by test and by the wire contract.
    pub fn service(&self) -> String {
        match self {
            Self::PostProcess { provider_id } => format!("post-process/{provider_id}"),
            Self::NotesLlm { provider, origin } => {
                format!("notes-llm/{}/{origin}", provider.as_str())
            }
            Self::NotesAcp { vault_id, origin } => format!("notes-acp/{vault_id}/{origin}"),
        }
    }

    /// The origin a request carrying this slot's secret must match, or `None`
    /// for the app's own post-processing key, which is not proxied.
    pub fn origin(&self) -> Option<&str> {
        match self {
            Self::PostProcess { .. } => None,
            Self::NotesLlm { origin, .. } | Self::NotesAcp { origin, .. } => Some(origin),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CredentialStatus {
    Configured,
    Missing,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// Secure storage could not be reached: a locked keychain, no Secret
    /// Service on a headless Linux box, a platform failure. Never a reason to
    /// fall back to keeping the secret somewhere else.
    Unavailable(String),
    /// The request itself was wrong: a blank secret, or an origin that is not
    /// `scheme://host[:port]`.
    Invalid(String),
}

impl fmt::Display for CredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => {
                write!(formatter, "secure storage unavailable: {message}")
            }
            Self::Invalid(message) => write!(formatter, "invalid credential request: {message}"),
        }
    }
}

impl std::error::Error for CredentialError {}

/// Where secrets are kept. `service` is always `CredentialSlot::service()`.
pub trait SecretBackend: Send + Sync {
    fn get(&self, service: &str) -> Result<Option<String>, CredentialError>;
    fn set(&self, service: &str, secret: &str) -> Result<(), CredentialError>;
    /// Deleting an entry that is not there succeeds.
    fn delete(&self, service: &str) -> Result<(), CredentialError>;
}

pub struct CredentialStore {
    /// One mutex over the whole backend. The Windows store's README warns
    /// against operating on one entry from two threads at once, and
    /// credential operations are rare and user-initiated, so finer-grained
    /// locking would buy nothing.
    backend: Mutex<Box<dyn SecretBackend>>,
}

impl CredentialStore {
    /// The production store, backed by the OS credential store.
    /// [`install_platform_store`] must have run first.
    pub fn keyring() -> Self {
        Self::with_backend(Box::new(KeyringBackend))
    }

    pub fn with_backend(backend: Box<dyn SecretBackend>) -> Self {
        Self {
            backend: Mutex::new(backend),
        }
    }

    /// Stores `secret` with surrounding whitespace removed: a pasted API key
    /// usually arrives with a trailing newline, and providers reject it.
    pub fn set(&self, slot: &CredentialSlot, secret: &str) -> Result<(), CredentialError> {
        let secret = secret.trim();
        if secret.is_empty() {
            return Err(CredentialError::Invalid("the secret is empty".into()));
        }
        lock(&self.backend).set(&slot.service(), secret)
    }

    pub fn clear(&self, slot: &CredentialSlot) -> Result<(), CredentialError> {
        lock(&self.backend).delete(&slot.service())
    }

    pub fn status(&self, slot: &CredentialSlot) -> CredentialStatus {
        match self.get(slot) {
            Ok(Some(_)) => CredentialStatus::Configured,
            Ok(None) => CredentialStatus::Missing,
            Err(error) => {
                log::debug!("credential status for {}: {error}", slot.service());
                CredentialStatus::Unavailable
            }
        }
    }

    /// Crate-private: a secret is only ever read in order to attach it to an
    /// outgoing request.
    pub(crate) fn get(&self, slot: &CredentialSlot) -> Result<Option<String>, CredentialError> {
        lock(&self.backend).get(&slot.service())
    }
}

/// A panic while one credential call held the lock must not disable every
/// later call. The guarded values are a backend handle and a map of strings,
/// neither of which a half-finished operation leaves inconsistent.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Deserializes an origin, rejecting anything that is not exactly
/// `scheme://host[:port]`.
fn origin_only<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let raw = String::deserialize(deserializer)?;
    canonical_origin(&raw).map_err(serde::de::Error::custom)
}

/// Canonicalises an origin as `Url::origin().ascii_serialization()`. The HTTP
/// proxy decides whether a request may carry a slot's secret by comparing the
/// request URL's origin with the slot's, so both strings have to come from
/// this same call: a hand-built `scheme://host:port` would differ over a
/// default port, letter case, or an IPv6 host's brackets.
pub fn canonical_origin(raw: &str) -> Result<String, CredentialError> {
    let url = Url::parse(raw)
        .map_err(|error| CredentialError::Invalid(format!("origin is not a URL: {error}")))?;
    // Every other scheme has an opaque origin, which no two URLs share, so a
    // slot keyed on one could never match a request.
    if !matches!(url.scheme(), "http" | "https" | "ws" | "wss") {
        return Err(CredentialError::Invalid(format!(
            "origin scheme `{}` is not http, https, ws or wss",
            url.scheme()
        )));
    }
    if url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(CredentialError::Invalid(
            "an origin is scheme://host[:port], with no path, query, fragment or userinfo".into(),
        ));
    }
    Ok(url.origin().ascii_serialization())
}

/// An in-process backend. Deliberately not `#[cfg(test)]`: the request-socket
/// tests (Task A4) live in another module and need a store they can drive.
#[derive(Debug, Default)]
pub struct MemoryBackend(Mutex<HashMap<String, String>>);

impl SecretBackend for MemoryBackend {
    fn get(&self, service: &str) -> Result<Option<String>, CredentialError> {
        Ok(lock(&self.0).get(service).cloned())
    }

    fn set(&self, service: &str, secret: &str) -> Result<(), CredentialError> {
        lock(&self.0).insert(service.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, service: &str) -> Result<(), CredentialError> {
        lock(&self.0).remove(service);
        Ok(())
    }
}

struct KeyringBackend;

impl SecretBackend for KeyringBackend {
    fn get(&self, service: &str) -> Result<Option<String>, CredentialError> {
        match entry(service)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(unavailable(error)),
        }
    }

    fn set(&self, service: &str, secret: &str) -> Result<(), CredentialError> {
        entry(service)?.set_password(secret).map_err(unavailable)
    }

    fn delete(&self, service: &str) -> Result<(), CredentialError> {
        match entry(service)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(error) => Err(unavailable(error)),
        }
    }
}

/// Called once from `lib.rs` before the store is managed. Selecting the store
/// explicitly is keyring-core's guidance for applications; `persistence=local`
/// keeps Windows entries on this machine rather than roaming with a domain
/// profile.
pub fn install_platform_store() -> Result<(), CredentialError> {
    #[cfg(windows)]
    {
        let store = windows_native_keyring_store::Store::new_with_configuration(&HashMap::from([
            ("prefix", "Shorthand/"),
            ("divider", "@"),
            ("service_no_divider", "true"),
        ]))
        .map_err(unavailable)?;
        keyring_core::set_default_store(store);
    }
    #[cfg(target_os = "macos")]
    {
        keyring_core::set_default_store(
            apple_native_keyring_store::keychain::Store::new().map_err(unavailable)?,
        );
    }
    #[cfg(target_os = "linux")]
    {
        keyring_core::set_default_store(
            zbus_secret_service_keyring_store::Store::new().map_err(unavailable)?,
        );
    }
    Ok(())
}

fn entry(service: &str) -> Result<keyring_core::Entry, CredentialError> {
    #[cfg(windows)]
    let modifiers = HashMap::from([("persistence", "local")]);
    #[cfg(not(windows))]
    let modifiers: HashMap<&str, &str> = HashMap::new();
    keyring_core::Entry::new_with_modifiers(service, KEYRING_USER, &modifiers).map_err(unavailable)
}

/// Every keyring failure that reaches a caller means "secure storage did not
/// answer". The platform's own message is kept because it is the only clue to
/// a locked keychain or a missing Secret Service, and it never contains the
/// secret.
fn unavailable(error: keyring_core::Error) -> CredentialError {
    CredentialError::Unavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> CredentialStore {
        CredentialStore::with_backend(Box::new(MemoryBackend::default()))
    }

    #[test]
    fn slot_service_strings_are_stable() {
        assert_eq!(
            CredentialSlot::PostProcess {
                provider_id: "openai".into()
            }
            .service(),
            "post-process/openai"
        );
        assert_eq!(
            CredentialSlot::NotesLlm {
                provider: LlmProvider::OpenaiCompatible,
                origin: "http://localhost:11434".into()
            }
            .service(),
            "notes-llm/openai-compatible/http://localhost:11434"
        );
        assert_eq!(
            CredentialSlot::NotesAcp {
                vault_id: "abc".into(),
                origin: "wss://agent.example".into()
            }
            .service(),
            "notes-acp/abc/wss://agent.example"
        );
    }

    #[test]
    fn set_status_clear_round_trip() {
        let store = store();
        let slot = CredentialSlot::NotesLlm {
            provider: LlmProvider::Openai,
            origin: "https://api.openai.com".into(),
        };
        assert!(matches!(store.status(&slot), CredentialStatus::Missing));
        store.set(&slot, "sk-test-1").unwrap();
        assert!(matches!(store.status(&slot), CredentialStatus::Configured));
        assert_eq!(store.get(&slot).unwrap().as_deref(), Some("sk-test-1"));
        store.clear(&slot).unwrap();
        assert!(matches!(store.status(&slot), CredentialStatus::Missing));
        store.clear(&slot).unwrap(); // clearing an absent entry is not an error
    }

    #[test]
    fn blank_secret_is_rejected_without_touching_the_backend() {
        let store = store();
        let slot = CredentialSlot::PostProcess {
            provider_id: "openai".into(),
        };
        store.set(&slot, "sk-test-1").unwrap();
        assert!(matches!(
            store.set(&slot, "   "),
            Err(CredentialError::Invalid(_))
        ));
        assert_eq!(store.get(&slot).unwrap().as_deref(), Some("sk-test-1"));
    }

    #[test]
    fn unavailable_backend_reports_unavailable_status() {
        struct Broken;
        impl SecretBackend for Broken {
            fn get(&self, _: &str) -> Result<Option<String>, CredentialError> {
                Err(CredentialError::Unavailable("locked".into()))
            }
            fn set(&self, _: &str, _: &str) -> Result<(), CredentialError> {
                Err(CredentialError::Unavailable("locked".into()))
            }
            fn delete(&self, _: &str) -> Result<(), CredentialError> {
                Err(CredentialError::Unavailable("locked".into()))
            }
        }
        let store = CredentialStore::with_backend(Box::new(Broken));
        let slot = CredentialSlot::PostProcess {
            provider_id: "openai".into(),
        };
        assert!(matches!(store.status(&slot), CredentialStatus::Unavailable));
    }

    #[test]
    fn origins_are_canonicalised_and_anything_wider_than_an_origin_is_rejected() {
        // The proxy compares a request URL's origin against the slot's by
        // comparing these strings, so the default port and the host's case
        // have to come out the same on both sides.
        assert_eq!(
            canonical_origin("https://API.OpenAI.com:443/").unwrap(),
            "https://api.openai.com"
        );
        assert_eq!(
            canonical_origin("http://localhost:11434").unwrap(),
            "http://localhost:11434"
        );
        for rejected in [
            "https://api.openai.com/v1",
            "https://api.openai.com/?key=x",
            "https://api.openai.com/#fragment",
            "https://user:pass@api.openai.com",
            "ftp://files.example.com",
            "not a url",
        ] {
            assert!(
                matches!(canonical_origin(rejected), Err(CredentialError::Invalid(_))),
                "{rejected} is wider than an origin and must be rejected"
            );
        }
    }

    #[test]
    fn slot_json_shape_matches_the_wire_contract() {
        let slot: CredentialSlot = serde_json::from_str(
            r#"{"kind":"notes-llm","provider":"anthropic","origin":"https://api.anthropic.com"}"#,
        )
        .unwrap();
        assert_eq!(
            slot.service(),
            "notes-llm/anthropic/https://api.anthropic.com"
        );
        assert!(
            serde_json::from_str::<CredentialSlot>(
                r#"{"kind":"notes-llm","provider":"openai","origin":"https://api.openai.com/v1"}"#
            )
            .is_err(),
            "origin must not carry a path"
        );
    }
}
