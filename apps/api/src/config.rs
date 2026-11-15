//! Server configuration, read once from the environment at start-up.
//! Secrets (database password, provider keys) are never logged.

use quorum_providers::{ApiKey, ProviderConfig, ProviderId, ProviderSettings};
use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// Connection string for the least-privileged application role.
    pub database_url: String,
    pub database_max_connections: u32,
    /// Set `Secure` on cookies and send HSTS. Must be true behind HTTPS.
    pub cookie_secure: bool,
    pub session_ttl: Duration,
    pub invitation_ttl: Duration,
    /// Whether anyone may self-register. Off unless explicitly enabled.
    pub allow_signup: bool,
    /// Directory with the built web client; served if present.
    pub web_dir: Option<PathBuf>,
    pub providers: ProviderSettings,
    /// Minimum interval between live provider requests by the same user.
    pub provider_cooldown: Duration,
    /// Maximum concurrent live provider requests across the server.
    pub provider_concurrency: usize,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("database_url", &"<redacted>")
            .field("cookie_secure", &self.cookie_secure)
            .field("allow_signup", &self.allow_signup)
            .field("web_dir", &self.web_dir)
            .field("live_providers", &self.providers.live_enabled)
            .field(
                "configured_providers",
                &self.providers.configs.keys().collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("{0} is invalid")]
    Invalid(&'static str),
}

fn flag(get: &impl Fn(&str) -> Option<String>, name: &str, default: bool) -> bool {
    match get(name).as_deref().map(str::trim) {
        Some("1" | "true" | "yes") => true,
        Some("0" | "false" | "no") => false,
        _ => default,
    }
}

fn number<T: std::str::FromStr>(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: T,
) -> Result<T, ConfigError> {
    match get(name) {
        Some(v) if !v.trim().is_empty() => v.trim().parse().map_err(|_| ConfigError::Invalid(name)),
        _ => Ok(default),
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Builds configuration from any key lookup (used by tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        // QUORUM_BIND wins; otherwise a platform-assigned PORT (e.g. Railway) binds all interfaces.
        let bind = match (
            get("QUORUM_BIND").filter(|v| !v.is_empty()),
            get("PORT").filter(|v| !v.is_empty()),
        ) {
            (Some(b), _) => b,
            (None, Some(port)) => format!("0.0.0.0:{}", port.trim()),
            (None, None) => "127.0.0.1:8080".into(),
        }
        .parse()
        .map_err(|_| ConfigError::Invalid("QUORUM_BIND"))?;
        let database_url = get("DATABASE_URL")
            .filter(|v| !v.is_empty())
            .ok_or(ConfigError::Missing("DATABASE_URL"))?;

        let live_enabled = flag(&get, "QUORUM_PROVIDERS_LIVE", false);
        let mut configs = BTreeMap::new();
        for p in ProviderId::ALL {
            let key = get(&format!("{}_API_KEY", p.env_prefix())).filter(|v| !v.trim().is_empty());
            let model = get(&format!("{}_MODEL", p.env_prefix())).filter(|v| !v.trim().is_empty());
            if let (Some(key), Some(model)) = (key, model) {
                configs.insert(
                    p,
                    ProviderConfig {
                        api_key: ApiKey::new(key.trim().to_owned()),
                        model: model.trim().to_owned(),
                        base_url: p.default_base_url().to_owned(),
                    },
                );
            }
        }

        Ok(Self {
            bind,
            database_url,
            database_max_connections: number(&get, "QUORUM_DB_MAX_CONNECTIONS", 10)?,
            cookie_secure: flag(&get, "QUORUM_COOKIE_SECURE", true),
            session_ttl: Duration::from_secs(
                number(&get, "QUORUM_SESSION_TTL_HOURS", 12u64)?.saturating_mul(3600),
            ),
            invitation_ttl: Duration::from_secs(
                number(&get, "QUORUM_INVITATION_TTL_HOURS", 72u64)?.saturating_mul(3600),
            ),
            allow_signup: flag(&get, "QUORUM_ALLOW_SIGNUP", false),
            web_dir: get("QUORUM_WEB_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            providers: ProviderSettings {
                live_enabled,
                configs,
                max_output_tokens: number(&get, "QUORUM_PROVIDER_MAX_OUTPUT_TOKENS", 1200)?,
                timeout: Duration::from_secs(number(&get, "QUORUM_PROVIDER_TIMEOUT_SECONDS", 45)?),
            },
            provider_cooldown: Duration::from_secs(number(
                &get,
                "QUORUM_PROVIDER_COOLDOWN_SECONDS",
                5,
            )?),
            provider_concurrency: number(&get, "QUORUM_PROVIDER_CONCURRENCY", 4)?,
        })
    }

    pub fn session_cookie_name(&self) -> &'static str {
        // The __Host- prefix requires Secure; fall back for plain-HTTP local development.
        if self.cookie_secure {
            "__Host-quorum_session"
        } else {
            "quorum_session"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(move |k| map.get(k).cloned())
    }

    #[test]
    fn secure_defaults() {
        let c = cfg(&[("DATABASE_URL", "postgres://x")]).expect("config");
        assert!(c.cookie_secure);
        assert!(!c.allow_signup);
        assert!(!c.providers.live_enabled);
        assert!(c.providers.configs.is_empty());
        assert_eq!(c.bind.to_string(), "127.0.0.1:8080");
        assert_eq!(c.session_cookie_name(), "__Host-quorum_session");
    }

    #[test]
    fn platform_port_is_honoured_unless_bind_is_set() {
        let c = cfg(&[("DATABASE_URL", "x"), ("PORT", "3000")]).expect("config");
        assert_eq!(c.bind.to_string(), "0.0.0.0:3000");
        let c = cfg(&[
            ("DATABASE_URL", "x"),
            ("PORT", "3000"),
            ("QUORUM_BIND", "127.0.0.1:9000"),
        ])
        .expect("config");
        assert_eq!(c.bind.to_string(), "127.0.0.1:9000");
        assert!(cfg(&[("DATABASE_URL", "x"), ("PORT", "abc")]).is_err());
    }

    #[test]
    fn database_url_is_required() {
        assert!(matches!(
            cfg(&[]),
            Err(ConfigError::Missing("DATABASE_URL"))
        ));
    }

    #[test]
    fn provider_needs_both_key_and_model() {
        let c = cfg(&[
            ("DATABASE_URL", "postgres://x"),
            ("QUORUM_PROVIDERS_LIVE", "1"),
            ("OPENAI_API_KEY", "k"),
            ("ANTHROPIC_API_KEY", "k"),
            ("ANTHROPIC_MODEL", "m"),
        ])
        .expect("config");
        assert!(!c.providers.configs.contains_key(&ProviderId::Openai));
        assert!(c.providers.configs.contains_key(&ProviderId::Anthropic));
    }

    #[test]
    fn debug_output_redacts_secrets() {
        let c = cfg(&[
            ("DATABASE_URL", "postgres://user:hunter2@db/x"),
            ("XAI_API_KEY", "xai-secret"),
            ("XAI_MODEL", "m"),
        ])
        .expect("config");
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("hunter2"));
        assert!(!dbg.contains("xai-secret"));
    }

    #[test]
    fn invalid_numbers_are_rejected() {
        assert!(cfg(&[("DATABASE_URL", "x"), ("QUORUM_DB_MAX_CONNECTIONS", "many")]).is_err());
    }
}
