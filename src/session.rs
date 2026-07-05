use std::error::Error as StdError;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::oauth::TwitchAuthOutcome;

pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;
pub type StoreResult<T> = Result<T, BoxError>;

#[async_trait]
pub trait TwitchAuthStore: Send + Sync {
    async fn load_auth(&self) -> StoreResult<Option<TwitchAuthOutcome>>;

    async fn save_auth(&self, auth: &TwitchAuthOutcome) -> StoreResult<()>;

    async fn clear_auth(&self) -> StoreResult<()>;
}

#[derive(Clone, Default)]
pub struct InMemoryTwitchAuthStore {
    state: Arc<Mutex<Option<TwitchAuthOutcome>>>,
}

impl InMemoryTwitchAuthStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_auth(auth: TwitchAuthOutcome) -> Self {
        Self {
            state: Arc::new(Mutex::new(Some(auth))),
        }
    }
}

#[async_trait]
impl TwitchAuthStore for InMemoryTwitchAuthStore {
    async fn load_auth(&self) -> StoreResult<Option<TwitchAuthOutcome>> {
        Ok(self
            .state
            .lock()
            .expect("in-memory auth store lock poisoned")
            .clone())
    }

    async fn save_auth(&self, auth: &TwitchAuthOutcome) -> StoreResult<()> {
        *self
            .state
            .lock()
            .expect("in-memory auth store lock poisoned") = Some(auth.clone());
        Ok(())
    }

    async fn clear_auth(&self) -> StoreResult<()> {
        *self
            .state
            .lock()
            .expect("in-memory auth store lock poisoned") = None;
        Ok(())
    }
}

#[cfg(feature = "reqwest-client")]
fn store_err(context: &'static str, err: BoxError) -> anyhow::Error {
    anyhow::anyhow!("{context}: {err}")
}

#[cfg(feature = "reqwest-client")]
pub async fn ensure_valid_stored_auth<S: TwitchAuthStore>(
    http: &reqwest::Client,
    store: &S,
    client_id: &str,
) -> anyhow::Result<Option<TwitchAuthOutcome>> {
    let Some(mut auth) = store
        .load_auth()
        .await
        .map_err(|err| store_err("failed to load stored auth", err))?
    else {
        return Ok(None);
    };

    if crate::native::validate_access_token(http, &auth.tokens.access_token)
        .await?
        .is_some()
    {
        return Ok(Some(auth));
    }

    let Some(refreshed) =
        crate::native::refresh_access_token(http, client_id, &auth.tokens.refresh_token).await?
    else {
        store
            .clear_auth()
            .await
            .map_err(|err| store_err("failed to clear rejected auth", err))?;
        return Ok(None);
    };

    auth.tokens = refreshed;
    store
        .save_auth(&auth)
        .await
        .map_err(|err| store_err("failed to persist refreshed auth", err))?;
    Ok(Some(auth))
}

#[cfg(feature = "reqwest-client")]
pub async fn refresh_stored_auth<S: TwitchAuthStore>(
    http: &reqwest::Client,
    store: &S,
    client_id: &str,
) -> anyhow::Result<Option<TwitchAuthOutcome>> {
    let Some(mut auth) = store
        .load_auth()
        .await
        .map_err(|err| store_err("failed to load stored auth", err))?
    else {
        return Ok(None);
    };

    let Some(refreshed) =
        crate::native::refresh_access_token(http, client_id, &auth.tokens.refresh_token).await?
    else {
        store
            .clear_auth()
            .await
            .map_err(|err| store_err("failed to clear rejected auth", err))?;
        return Ok(None);
    };

    auth.tokens = refreshed;
    store
        .save_auth(&auth)
        .await
        .map_err(|err| store_err("failed to persist refreshed auth", err))?;
    Ok(Some(auth))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TwitchIdentity;
    use crate::oauth::{TwitchAuthOutcome, TwitchTokenState};

    fn sample_auth() -> TwitchAuthOutcome {
        TwitchAuthOutcome {
            identity: TwitchIdentity::new("42", "viewer", "Viewer"),
            tokens: TwitchTokenState {
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                expires_in_seconds: Some(3600),
                scope: vec!["chat:read".to_string()],
                token_type: "bearer".to_string(),
                linked_at_ms: 1_700_000_000_000,
            },
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn in_memory_auth_store_round_trips() {
        let store = InMemoryTwitchAuthStore::new();
        assert!(
            store
                .load_auth()
                .await
                .expect("load should succeed")
                .is_none()
        );

        let auth = sample_auth();
        store.save_auth(&auth).await.expect("save should succeed");
        assert_eq!(
            store.load_auth().await.expect("load should succeed"),
            Some(auth.clone())
        );

        store.clear_auth().await.expect("clear should succeed");
        assert!(
            store
                .load_auth()
                .await
                .expect("load should succeed")
                .is_none()
        );
    }
}
