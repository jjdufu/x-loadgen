//! Single-flight plaintext token refresh with exponential backoff.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use tokio::sync::Mutex;

/// Cached plaintext token and the instant at which it should be refreshed.
#[derive(Debug, Clone)]
pub struct CachedToken {
    pub value: String,
    pub expires_at: Instant,
}

/// Failure to obtain a plaintext token.
#[derive(Debug, Clone, thiserror::Error)]
#[error("token refresh failed: {0}")]
pub struct TokenError(pub String);

/// Async refresh callback. Implementations must not themselves serialize callers;
/// [`TokenBroker`] provides single-flight.
pub type RefreshFn =
    Arc<dyn Fn() -> BoxFuture<'static, Result<CachedToken, TokenError>> + Send + Sync>;

/// Shares one in-flight refresh among concurrent request tasks.
pub struct TokenBroker {
    refresh: RefreshFn,
    state: Mutex<BrokerState>,
    max_backoff: Duration,
}

struct BrokerState {
    cached: Option<CachedToken>,
    backoff: Duration,
    retry_after: Option<Instant>,
    refresh_count: u64,
}

impl TokenBroker {
    /// Create a broker. The first [`TokenBroker::get`] will refresh immediately.
    #[must_use]
    pub fn new(refresh: RefreshFn) -> Self {
        Self::with_backoff(refresh, Duration::from_secs(5))
    }

    /// Create a broker with a custom backoff ceiling (used by tests).
    #[must_use]
    pub fn with_backoff(refresh: RefreshFn, max_backoff: Duration) -> Self {
        Self {
            refresh,
            state: Mutex::new(BrokerState {
                cached: None,
                backoff: Duration::from_millis(50),
                retry_after: None,
                refresh_count: 0,
            }),
            max_backoff,
        }
    }

    /// How many times the refresh callback has completed (success or failure).
    pub async fn refresh_count(&self) -> u64 {
        self.state.lock().await.refresh_count
    }

    /// Return a valid token, refreshing at most once at a time.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError`] when no cached token is usable and refresh failed
    /// or is delayed by backoff. The caller should record `error_class=token_refresh`
    /// and keep running.
    pub async fn get(&self) -> Result<String, TokenError> {
        let mut state = self.state.lock().await;
        if let Some(token) = state.cached.as_ref()
            && Instant::now() < token.expires_at
        {
            return Ok(token.value.clone());
        }
        if let Some(retry_after) = state.retry_after
            && Instant::now() < retry_after
        {
            return Err(TokenError(
                "token refresh backing off after a previous failure".into(),
            ));
        }
        let refresh = Arc::clone(&self.refresh);
        // Hold the mutex for the duration of the refresh so concurrent callers
        // wait here instead of issuing a second refresh (single-flight).
        let result = refresh().await;
        state.refresh_count = state.refresh_count.saturating_add(1);
        match result {
            Ok(token) => {
                state.backoff = Duration::from_millis(50);
                state.retry_after = None;
                let value = token.value.clone();
                state.cached = Some(token);
                Ok(value)
            }
            Err(err) => {
                let backoff = state.backoff;
                state.retry_after = Some(Instant::now() + backoff);
                state.backoff = (backoff * 2).min(self.max_backoff);
                state.cached = None;
                Err(err)
            }
        }
    }
}

/// Build a boxed refresh future from an async closure.
pub fn box_refresh<F, Fut>(func: F) -> RefreshFn
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<CachedToken, TokenError>> + Send + 'static,
{
    Arc::new(move || Box::pin(func()))
}
