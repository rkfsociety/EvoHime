//! Shared Postgres helpers for integration tests (Stage 7.84).
//!
//! Locally, missing Postgres skips the test (returns `None`).
//! In CI (`CI` set) or when `EVOHIME_REQUIRE_DB=1`, connection/migrate
//! failure panics so the suite cannot silently pass without a database.

use sqlx::PgPool;

const DEFAULT_DATABASE_URL: &str = "postgres://evohime:evohime@localhost:5432/evohime";

pub fn integration_database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.into())
}

/// `true` when integration tests must not soft-skip.
pub fn require_integration_database() -> bool {
    if std::env::var_os("CI").is_some() {
        return true;
    }
    matches!(
        std::env::var("EVOHIME_REQUIRE_DB").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

/// Connect + run migrations for integration tests.
///
/// Returns `None` when the database is unavailable and not required.
/// Panics when the database is required and connect/migrate fails.
pub async fn connect_integration_pool() -> Option<PgPool> {
    let url = integration_database_url();
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(err) => {
            if require_integration_database() {
                panic!("integration database required but connect failed ({url}): {err}");
            }
            return None;
        }
    };

    match crate::run_migrations(&pool).await {
        Ok(()) => Some(pool),
        Err(err) => {
            if require_integration_database() {
                panic!("integration database required but migrations failed: {err}");
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_url_matches_local_dev() {
        // Avoid env races: only assert the constant used as fallback.
        assert!(DEFAULT_DATABASE_URL.contains("evohime"));
        assert!(DEFAULT_DATABASE_URL.contains("5432"));
    }

    #[test]
    fn require_db_flag_parses_truthy_values() {
        fn is_truthy(value: &str) -> bool {
            matches!(value, "1" | "true" | "TRUE" | "yes" | "YES")
        }
        assert!(is_truthy("1"));
        assert!(is_truthy("true"));
        assert!(!is_truthy("0"));
        assert!(!is_truthy("false"));
    }
}
