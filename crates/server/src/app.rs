use sqlx::PgPool;
use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub bind_addr: String,
    pub demo_file_path: PathBuf,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://evohime:evohime@localhost:5432/evohime".to_string());
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
        let demo_file_path = env::var("DEMO_FILE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/sample-context.md"));

        Self {
            database_url,
            bind_addr,
            demo_file_path,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub demo_file_path: PathBuf,
}
