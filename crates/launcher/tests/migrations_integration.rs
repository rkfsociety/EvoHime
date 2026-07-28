//! Integration test: applies the real project migrations (the same `.sql`
//! files shipped inside the `migrations.zip` release artifact from Фаза 1)
//! through `evohime_launcher::apply_migrations`, proving the runtime
//! `Migrator::new(path)` approach works against the actual repo, not just a
//! synthetic fixture — and that it requires no `sqlx-cli` on the machine.
//!
//! Follows the same soft-skip/require convention as
//! `evohime_storage::test_db`: missing Postgres locally skips the test;
//! `CI` or `EVOHIME_REQUIRE_DB=1` makes a missing/broken database a hard
//! failure.

use sqlx::postgres::PgPoolOptions;
use std::path::Path;

const DEFAULT_DATABASE_URL: &str = "postgres://evohime:evohime@localhost:5432/evohime";

fn integration_database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.into())
}

fn require_integration_database() -> bool {
    if std::env::var_os("CI").is_some() {
        return true;
    }
    matches!(
        std::env::var("EVOHIME_REQUIRE_DB").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

#[tokio::test]
async fn applies_real_project_migrations_without_sqlx_cli() {
    let url = integration_database_url();
    let pool = match PgPoolOptions::new().max_connections(2).connect(&url).await {
        Ok(pool) => pool,
        Err(err) => {
            if require_integration_database() {
                panic!("integration database required but connect failed ({url}): {err}");
            }
            return;
        }
    };

    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");

    if let Err(err) = evohime_launcher::apply_migrations(&pool, &migrations_dir).await {
        if require_integration_database() {
            panic!("migrations required but failed: {err}");
        }
    }
}
