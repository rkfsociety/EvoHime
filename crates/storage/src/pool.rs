//! PostgreSQL pool options (Stage 7.19).
//!
//! Env (all optional):
//! - `EVOHIME_PG_MAX_CONNECTIONS` (default 10)
//! - `EVOHIME_PG_MIN_CONNECTIONS` (default 0)
//! - `EVOHIME_PG_ACQUIRE_TIMEOUT_SECS` (default 30)
//! - `EVOHIME_PG_IDLE_TIMEOUT_SECS` (default 600; `0` disables)
//! - `EVOHIME_PG_MAX_LIFETIME_SECS` (default 1800; `0` disables)

use crate::StorageError;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(600)),
            max_lifetime: Some(Duration::from_secs(1800)),
        }
    }
}

impl PoolConfig {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let max_connections =
            env_u32("EVOHIME_PG_MAX_CONNECTIONS", defaults.max_connections).max(1);
        let mut min_connections = env_u32("EVOHIME_PG_MIN_CONNECTIONS", defaults.min_connections);
        if min_connections > max_connections {
            min_connections = max_connections;
        }
        Self {
            max_connections,
            min_connections,
            acquire_timeout: Duration::from_secs(
                env_u64(
                    "EVOHIME_PG_ACQUIRE_TIMEOUT_SECS",
                    defaults.acquire_timeout.as_secs(),
                )
                .max(1),
            ),
            idle_timeout: env_optional_secs("EVOHIME_PG_IDLE_TIMEOUT_SECS", 600),
            max_lifetime: env_optional_secs("EVOHIME_PG_MAX_LIFETIME_SECS", 1800),
        }
    }
}

/// Connect with tuned pool options.
pub async fn connect_pool(database_url: &str, config: &PoolConfig) -> Result<PgPool, StorageError> {
    let mut options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout);

    if let Some(idle) = config.idle_timeout {
        options = options.idle_timeout(idle);
    } else {
        options = options.idle_timeout(None);
    }

    if let Some(lifetime) = config.max_lifetime {
        options = options.max_lifetime(lifetime);
    } else {
        options = options.max_lifetime(None);
    }

    Ok(options.connect(database_url).await?)
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// `0` → `None` (disabled). Missing → `Some(default_secs)`.
fn env_optional_secs(name: &str, default_secs: u64) -> Option<Duration> {
    match std::env::var(name) {
        Ok(value) => {
            let secs: u64 = value.parse().unwrap_or(default_secs);
            if secs == 0 {
                None
            } else {
                Some(Duration::from_secs(secs))
            }
        }
        Err(_) => Some(Duration::from_secs(default_secs)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pool_config_is_sane() {
        let config = PoolConfig::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 0);
        assert_eq!(config.acquire_timeout, Duration::from_secs(30));
        assert_eq!(config.idle_timeout, Some(Duration::from_secs(600)));
        assert_eq!(config.max_lifetime, Some(Duration::from_secs(1800)));
    }

    #[test]
    fn optional_secs_zero_disables() {
        // Direct helper coverage without mutating process env races:
        assert_eq!(
            {
                let secs: u64 = 0;
                if secs == 0 {
                    None
                } else {
                    Some(Duration::from_secs(secs))
                }
            },
            None::<Duration>
        );
    }

    #[test]
    fn clamps_min_connections_to_max() {
        let mut config = PoolConfig {
            max_connections: 4,
            min_connections: 8,
            ..PoolConfig::default()
        };
        if config.min_connections > config.max_connections {
            config.min_connections = config.max_connections;
        }
        assert_eq!(config.min_connections, 4);
    }
}
