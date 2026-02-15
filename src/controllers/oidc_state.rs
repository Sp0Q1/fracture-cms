use openidconnect::{
    core::CoreClient, CsrfToken, EndpointMaybeSet, EndpointNotSet, EndpointSet, Nonce,
    PkceCodeVerifier,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const TTL: Duration = Duration::from_secs(300);

/// Type alias for a `CoreClient` after OIDC discovery + redirect URI set.
pub type DiscoveredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

pub struct PendingAuth {
    pub pkce_verifier: PkceCodeVerifier,
    pub nonce: Nonce,
    pub created_at: Instant,
}

#[derive(Clone)]
pub struct OidcStateStore {
    inner: Arc<Mutex<HashMap<String, PendingAuth>>>,
    ttl: Duration,
}

impl OidcStateStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl: TTL,
        }
    }

    pub fn insert(&self, csrf_token: &CsrfToken, pending: PendingAuth) {
        let mut map = self.inner.lock().expect("lock poisoned");
        let ttl = self.ttl;
        // Evict expired entries
        map.retain(|_, v| v.created_at.elapsed() < ttl);
        map.insert(csrf_token.secret().clone(), pending);
    }

    pub fn take(&self, csrf_state: &str) -> Option<PendingAuth> {
        let mut map = self.inner.lock().expect("lock poisoned");
        let pending = map.remove(csrf_state)?;
        if pending.created_at.elapsed() >= self.ttl {
            return None;
        }
        Some(pending)
    }

    #[cfg(test)]
    fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().expect("lock poisoned").len()
    }
}

#[derive(Clone)]
pub struct OidcContext {
    pub client: DiscoveredClient,
    pub state_store: OidcStateStore,
    pub provider_name: String,
    pub scopes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn make_pending() -> PendingAuth {
        PendingAuth {
            pkce_verifier: PkceCodeVerifier::new("test-verifier".to_string()),
            nonce: Nonce::new("test-nonce".to_string()),
            created_at: Instant::now(),
        }
    }

    fn make_expired_pending() -> PendingAuth {
        PendingAuth {
            pkce_verifier: PkceCodeVerifier::new("expired-verifier".to_string()),
            nonce: Nonce::new("expired-nonce".to_string()),
            // Created far in the past
            created_at: Instant::now() - Duration::from_secs(600),
        }
    }

    #[test]
    fn insert_and_take_valid_entry() {
        let store = OidcStateStore::new();
        let token = CsrfToken::new("csrf-123".to_string());

        store.insert(&token, make_pending());
        let pending = store.take("csrf-123");

        assert!(pending.is_some());
        let p = pending.unwrap();
        assert_eq!(p.nonce.secret(), "test-nonce");
    }

    #[test]
    fn take_removes_entry() {
        let store = OidcStateStore::new();
        let token = CsrfToken::new("csrf-456".to_string());

        store.insert(&token, make_pending());

        assert!(store.take("csrf-456").is_some());
        assert!(store.take("csrf-456").is_none());
    }

    #[test]
    fn take_returns_none_for_unknown_state() {
        let store = OidcStateStore::new();
        assert!(store.take("nonexistent").is_none());
    }

    #[test]
    fn take_returns_none_for_expired_entry() {
        let store = OidcStateStore::new();
        let token = CsrfToken::new("csrf-expired".to_string());

        store.insert(&token, make_expired_pending());

        assert!(store.take("csrf-expired").is_none());
    }

    #[test]
    fn insert_evicts_expired_entries() {
        let store = OidcStateStore::new();

        let expired_token = CsrfToken::new("old".to_string());
        store.insert(&expired_token, make_expired_pending());
        // Force it into the map despite being "expired" by direct insertion
        {
            let mut map = store.inner.lock().unwrap();
            map.insert(
                "old".to_string(),
                make_expired_pending(),
            );
        }
        assert_eq!(store.len(), 1);

        // Inserting a new entry should evict the expired one
        let new_token = CsrfToken::new("new".to_string());
        store.insert(&new_token, make_pending());

        assert_eq!(store.len(), 1);
        assert!(store.take("old").is_none());
        assert!(store.take("new").is_some());
    }

    #[test]
    fn ttl_expiration_with_short_ttl() {
        let store = OidcStateStore::with_ttl(Duration::from_millis(50));
        let token = CsrfToken::new("short-lived".to_string());

        store.insert(&token, make_pending());
        assert!(store.take("short-lived").is_some());

        // Insert again and wait for expiration
        store.insert(&token, make_pending());
        thread::sleep(Duration::from_millis(60));
        assert!(store.take("short-lived").is_none());
    }

    #[test]
    fn store_is_clone_and_shared() {
        let store = OidcStateStore::new();
        let store2 = store.clone();
        let token = CsrfToken::new("shared".to_string());

        store.insert(&token, make_pending());
        // The clone should see the same data
        assert!(store2.take("shared").is_some());
    }
}
