use crate::config::Config;
use crate::throttle::Throttle;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

pub struct Inner {
    pub config: Config,
    pub pool: PgPool,
    pub http: reqwest::Client,
    /// Failed sign-in attempts per normalized email.
    pub login_throttle: Throttle,
    /// Live provider requests per user (cooldown).
    pub provider_throttle: Throttle,
    pub provider_slots: Semaphore,
}

impl AppState {
    pub fn new(config: Config, pool: PgPool) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .user_agent(concat!("serk-quorum/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self::with_http_client(config, pool, http))
    }

    /// Allows tests to supply a client that may reach a local mock server.
    pub fn with_http_client(config: Config, pool: PgPool, http: reqwest::Client) -> Self {
        let slots = config.provider_concurrency.max(1);
        let cooldown = config.provider_cooldown;
        Self {
            inner: Arc::new(Inner {
                login_throttle: Throttle::new(5, std::time::Duration::from_secs(15 * 60)),
                provider_throttle: Throttle::new(1, cooldown),
                provider_slots: Semaphore::new(slots),
                config,
                pool,
                http,
            }),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }
}
