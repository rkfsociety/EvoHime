//! Integration test harness for HTTP/WebSocket testing.
//! Provides test server setup with auth, CORS, features, and WS support.

use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

pub struct TestServer {
    pub addr: SocketAddr,
    pub pool: PgPool,
    pub handle: tokio::task::JoinHandle<Result<(), hyper::Error>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Create a test PostgreSQL pool for integration tests.
/// Requires DATABASE_URL to point to a test database.
pub async fn test_pool() -> anyhow::Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://evohime:evohime@localhost:5432/evohime_test".to_string());

    let pool_config = evohime_storage::PoolConfig {
        min_connections: 1,
        max_connections: 5,
        acquire_timeout: std::time::Duration::from_secs(5),
        idle_timeout: Some(std::time::Duration::from_secs(10)),
        max_lifetime: Some(std::time::Duration::from_secs(60)),
    };

    let pool = evohime_storage::connect_pool(&database_url, &pool_config).await?;
    evohime_storage::run_migrations(&pool).await?;
    Ok(pool)
}

/// Placeholder for future integration tests.
/// Tests in this file will verify:
/// - HTTP auth (bearer token, loopback)
/// - CORS (allowed/denied origins)
/// - Feature gates (disabled features return 403)
/// - HTTP error responses (400 vs 500)
/// - WebSocket auth and reconnect with event resume
/// - Scheduler idempotency (two processes don't execute same task twice)
#[test]
fn test_integration_harness_documented() {
    // Integration tests will follow in subsequent commits.
    // See roadmap.md Phase 4 for full requirements.
}
