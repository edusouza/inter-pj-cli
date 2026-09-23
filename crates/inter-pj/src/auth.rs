//! OAuth2 *client credentials* tokens and their caching.
//!
//! Tokens last one hour and the token endpoint accepts only five calls per
//! minute, so tokens are reused while they are valid and cover the scopes an
//! operation needs. Besides the in-memory cache, a [`TokenStore`] can persist
//! tokens between processes (the CLI stores them in a user-only file).

use std::fmt::{self, Write as _};
use std::future::Future;
use std::io;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::error::Result;
use crate::scope::ScopeSet;

/// Tokens that expire in less than this are not reused.
pub const EXPIRY_MARGIN: TimeDelta = TimeDelta::seconds(60);

/// An OAuth access token, with the scopes it was granted and its expiry.
///
/// The [`Debug`] implementation never reveals the token. Serializing it (for
/// a [`TokenStore`]) does, on purpose.
#[derive(Clone)]
pub struct AccessToken {
    secret: SecretString,
    scopes: ScopeSet,
    expires_at: DateTime<Utc>,
}

impl AccessToken {
    /// Creates a token.
    pub fn new(secret: impl Into<String>, scopes: ScopeSet, expires_at: DateTime<Utc>) -> Self {
        Self {
            secret: SecretString::from(secret.into()),
            scopes,
            expires_at,
        }
    }

    /// The bearer token. Use [`ExposeSecret::expose_secret`] to read it.
    pub fn secret(&self) -> &SecretString {
        &self.secret
    }

    /// Scopes granted to the token.
    pub fn scopes(&self) -> &ScopeSet {
        &self.scopes
    }

    /// When the token expires.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Whether the token is expired (or about to expire) at `now`.
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now + EXPIRY_MARGIN
    }

    /// Whether the token can be used at `now` for an operation that needs `scopes`.
    pub fn covers(&self, scopes: &ScopeSet, now: DateTime<Utc>) -> bool {
        !self.is_expired_at(now) && self.scopes.is_superset(scopes)
    }

    fn same_as(&self, other: &AccessToken) -> bool {
        self.secret.expose_secret() == other.secret.expose_secret()
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessToken")
            .field("secret", &"[REDACTED]")
            .field("scopes", &self.scopes.to_string())
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
    scope: ScopeSet,
    expires_at: DateTime<Utc>,
}

impl Serialize for AccessToken {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        StoredToken {
            access_token: self.secret.expose_secret().to_owned(),
            scope: self.scopes.clone(),
            expires_at: self.expires_at,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AccessToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let stored = StoredToken::deserialize(deserializer)?;
        Ok(Self::new(
            stored.access_token,
            stored.scope,
            stored.expires_at,
        ))
    }
}

/// Persistent storage for access tokens, shared between processes.
///
/// Implementations must protect the stored tokens (e.g. files readable only
/// by the current user). Errors are not fatal: the client logs them and goes
/// on without the persistent cache.
pub trait TokenStore: Send + Sync + fmt::Debug {
    /// Loads the tokens stored under `key` (an empty list when there are none).
    ///
    /// # Errors
    ///
    /// Returns an error when the storage cannot be read or its content is invalid.
    fn load(&self, key: &str) -> io::Result<Vec<AccessToken>>;

    /// Replaces the tokens stored under `key`.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage cannot be written.
    fn save(&self, key: &str, tokens: &[AccessToken]) -> io::Result<()>;
}

/// Key identifying the tokens of an integration (base URL + `client_id`).
///
/// It is a hash, so that the `client_id` does not show up in file names.
pub fn cache_key(base_url: &str, client_id: &str) -> String {
    let digest = Sha256::digest(format!("{}\n{client_id}", base_url.trim_end_matches('/')));
    digest
        .iter()
        .take(16)
        .fold(String::with_capacity(32), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Caches tokens in memory and, optionally, in a [`TokenStore`].
pub(crate) struct TokenManager {
    key: String,
    store: Option<Arc<dyn TokenStore>>,
    additional_scopes: ScopeSet,
    memory: Mutex<Vec<AccessToken>>,
}

impl TokenManager {
    pub(crate) fn new(
        key: String,
        store: Option<Arc<dyn TokenStore>>,
        additional_scopes: ScopeSet,
    ) -> Self {
        Self {
            key,
            store,
            additional_scopes,
            memory: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// Returns a token covering `required`, reusing a cached one unless
    /// `force_new` is set. `fetch` requests a new token for the given scopes.
    pub(crate) async fn token<F, Fut>(
        &self,
        required: &ScopeSet,
        force_new: bool,
        fetch: F,
    ) -> Result<AccessToken>
    where
        F: FnOnce(ScopeSet) -> Fut,
        Fut: Future<Output = Result<AccessToken>>,
    {
        // Holding the lock while fetching makes concurrent callers wait for
        // the same token instead of spending the token endpoint's rate limit.
        let mut memory = self.memory.lock().await;
        let now = Utc::now();
        memory.retain(|token| !token.is_expired_at(now));

        if !force_new {
            if let Some(token) = memory.iter().find(|token| token.covers(required, now)) {
                tracing::debug!(escopos = %token.scopes, "token reaproveitado (memória)");
                return Ok(token.clone());
            }
            merge(&mut memory, self.load_store(now));
            if let Some(token) = memory.iter().find(|token| token.covers(required, now)) {
                tracing::debug!(escopos = %token.scopes, "token reaproveitado (cache)");
                return Ok(token.clone());
            }
        }

        let requested = required.union(&self.additional_scopes);
        tracing::debug!(escopos = %requested, "solicitando novo token");
        let token = fetch(requested).await?;
        memory.push(token.clone());

        let mut persisted = self.load_store(now);
        persisted.push(token.clone());
        self.save_store(&persisted);
        Ok(token)
    }

    /// Forgets `token` (e.g. after the API rejected it).
    pub(crate) async fn invalidate(&self, token: &AccessToken) {
        let mut memory = self.memory.lock().await;
        memory.retain(|cached| !cached.same_as(token));
        if self.store.is_some() {
            let mut persisted = self.load_store(Utc::now());
            let before = persisted.len();
            persisted.retain(|cached| !cached.same_as(token));
            if persisted.len() != before {
                self.save_store(&persisted);
            }
        }
    }

    fn load_store(&self, now: DateTime<Utc>) -> Vec<AccessToken> {
        let Some(store) = &self.store else {
            return Vec::new();
        };
        match store.load(&self.key) {
            Ok(mut tokens) => {
                tokens.retain(|token| !token.is_expired_at(now));
                tokens
            }
            Err(err) => {
                tracing::warn!("não foi possível ler o cache de tokens: {err}");
                Vec::new()
            }
        }
    }

    fn save_store(&self, tokens: &[AccessToken]) {
        if let Some(store) = &self.store {
            let mut unique: Vec<AccessToken> = Vec::with_capacity(tokens.len());
            merge(&mut unique, tokens.to_vec());
            if let Err(err) = store.save(&self.key, &unique) {
                tracing::warn!("não foi possível gravar o cache de tokens: {err}");
            }
        }
    }
}

impl fmt::Debug for TokenManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenManager")
            .field("store", &self.store)
            .field("additional_scopes", &self.additional_scopes.to_string())
            .finish_non_exhaustive()
    }
}

fn merge(into: &mut Vec<AccessToken>, tokens: Vec<AccessToken>) {
    for token in tokens {
        if !into.iter().any(|existing| existing.same_as(&token)) {
            into.push(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::scope::Scope;

    #[derive(Debug, Default)]
    struct MemoryStore {
        tokens: StdMutex<Vec<AccessToken>>,
        fail: bool,
    }

    impl TokenStore for MemoryStore {
        fn load(&self, _key: &str) -> io::Result<Vec<AccessToken>> {
            if self.fail {
                return Err(io::Error::other("falha simulada"));
            }
            Ok(self.tokens.lock().unwrap().clone())
        }

        fn save(&self, _key: &str, tokens: &[AccessToken]) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("falha simulada"));
            }
            *self.tokens.lock().unwrap() = tokens.to_vec();
            Ok(())
        }
    }

    fn token(secret: &str, scopes: &str, minutes: i64) -> AccessToken {
        AccessToken::new(
            secret,
            scopes.parse().unwrap(),
            Utc::now() + TimeDelta::minutes(minutes),
        )
    }

    fn scopes(s: &str) -> ScopeSet {
        s.parse().unwrap()
    }

    #[tokio::test]
    async fn fetches_once_and_reuses_from_memory() {
        let manager = TokenManager::new("k".into(), None, ScopeSet::new());
        let calls = AtomicUsize::new(0);
        for _ in 0..3 {
            let got = manager
                .token(&scopes("extrato.read"), false, |requested| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async move {
                        Ok(AccessToken::new(
                            "t1",
                            requested,
                            Utc::now() + TimeDelta::hours(1),
                        ))
                    }
                })
                .await
                .unwrap();
            assert_eq!(got.secret().expose_secret(), "t1");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn reuses_superset_token_and_fetches_for_new_scope() {
        let store = Arc::new(MemoryStore::default());
        store
            .tokens
            .lock()
            .unwrap()
            .push(token("amplo", "extrato.read pix.read", 30));
        let manager = TokenManager::new("k".into(), Some(store.clone()), ScopeSet::new());

        let reused = manager
            .token(&scopes("pix.read"), false, |_| async {
                panic!("não deveria buscar")
            })
            .await
            .unwrap();
        assert_eq!(reused.secret().expose_secret(), "amplo");

        let fresh = manager
            .token(&scopes("cob.read"), false, |requested| async move {
                Ok(AccessToken::new(
                    "novo",
                    requested,
                    Utc::now() + TimeDelta::hours(1),
                ))
            })
            .await
            .unwrap();
        assert_eq!(fresh.secret().expose_secret(), "novo");
        assert_eq!(
            store.tokens.lock().unwrap().len(),
            2,
            "novo token persistido junto ao antigo"
        );
    }

    #[tokio::test]
    async fn ignores_tokens_close_to_expiry() {
        let store = Arc::new(MemoryStore::default());
        store
            .tokens
            .lock()
            .unwrap()
            .push(token("quase", "extrato.read", 0));
        let manager = TokenManager::new("k".into(), Some(store.clone()), ScopeSet::new());
        let got = manager
            .token(&scopes("extrato.read"), false, |requested| async move {
                Ok(AccessToken::new(
                    "renovado",
                    requested,
                    Utc::now() + TimeDelta::hours(1),
                ))
            })
            .await
            .unwrap();
        assert_eq!(got.secret().expose_secret(), "renovado");
        let persisted = store.tokens.lock().unwrap();
        assert_eq!(persisted.len(), 1, "token expirado removido do cache");
    }

    #[tokio::test]
    async fn requests_additional_scopes() {
        let manager = TokenManager::new("k".into(), None, scopes("pix.read"));
        let got = manager
            .token(&scopes("extrato.read"), false, |requested| async move {
                assert_eq!(requested.to_string(), "extrato.read pix.read");
                Ok(AccessToken::new(
                    "t",
                    requested,
                    Utc::now() + TimeDelta::hours(1),
                ))
            })
            .await
            .unwrap();
        assert!(got.scopes().contains(Scope::PixRead));
    }

    #[tokio::test]
    async fn force_new_bypasses_caches() {
        let manager = TokenManager::new("k".into(), None, ScopeSet::new());
        for expected in ["a", "b"] {
            let got = manager
                .token(&scopes("extrato.read"), true, |requested| async move {
                    Ok(AccessToken::new(
                        expected,
                        requested,
                        Utc::now() + TimeDelta::hours(1),
                    ))
                })
                .await
                .unwrap();
            assert_eq!(got.secret().expose_secret(), expected);
        }
    }

    #[tokio::test]
    async fn invalidate_removes_from_memory_and_store() {
        let store = Arc::new(MemoryStore::default());
        let manager = TokenManager::new("k".into(), Some(store.clone()), ScopeSet::new());
        let first = manager
            .token(&scopes("extrato.read"), false, |requested| async move {
                Ok(AccessToken::new(
                    "rejeitado",
                    requested,
                    Utc::now() + TimeDelta::hours(1),
                ))
            })
            .await
            .unwrap();
        manager.invalidate(&first).await;
        assert!(store.tokens.lock().unwrap().is_empty());
        let second = manager
            .token(&scopes("extrato.read"), false, |requested| async move {
                Ok(AccessToken::new(
                    "valido",
                    requested,
                    Utc::now() + TimeDelta::hours(1),
                ))
            })
            .await
            .unwrap();
        assert_eq!(second.secret().expose_secret(), "valido");
    }

    #[tokio::test]
    async fn store_failures_are_not_fatal() {
        let store = Arc::new(MemoryStore {
            fail: true,
            ..MemoryStore::default()
        });
        let manager = TokenManager::new("k".into(), Some(store), ScopeSet::new());
        let got = manager
            .token(&scopes("extrato.read"), false, |requested| async move {
                Ok(AccessToken::new(
                    "t",
                    requested,
                    Utc::now() + TimeDelta::hours(1),
                ))
            })
            .await
            .unwrap();
        assert_eq!(got.secret().expose_secret(), "t");
    }

    #[test]
    fn debug_hides_token() {
        let debug = format!("{:?}", token("super-secreto", "extrato.read", 10));
        assert!(!debug.contains("super-secreto"));
        assert!(debug.contains("extrato.read"));
    }

    #[test]
    fn serialization_round_trips() {
        let original = token("abc", "extrato.read pix.read", 10);
        let json = serde_json::to_string(&original).unwrap();
        let back: AccessToken = serde_json::from_str(&json).unwrap();
        assert!(back.same_as(&original));
        assert_eq!(back.scopes(), original.scopes());
        assert_eq!(back.expires_at(), original.expires_at());
    }

    #[test]
    fn cache_key_is_stable_and_hides_client_id() {
        let key = cache_key("https://cdpj.partners.bancointer.com.br/", "meu-client-id");
        assert_eq!(
            key,
            cache_key("https://cdpj.partners.bancointer.com.br", "meu-client-id")
        );
        assert_eq!(key.len(), 32);
        assert!(!key.contains("meu-client-id"));
        assert_ne!(
            key,
            cache_key("https://cdpj-sandbox.partners.uatinter.co", "meu-client-id")
        );
    }
}
