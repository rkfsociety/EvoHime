#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable_and_not_plaintext() {
        let token = "operator-secret";
        let hash = hash_operator_token(token);
        assert_eq!(hash, hash_operator_token(token));
        assert_ne!(hash, token);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn roles_parse_only_known_values() {
        assert_eq!(OperatorRole::parse("owner"), Some(OperatorRole::Owner));
        assert_eq!(OperatorRole::parse("member"), Some(OperatorRole::Member));
        assert_eq!(OperatorRole::parse("admin"), None);
    }

    #[test]
    fn last_owner_cannot_be_revoked() {
        assert!(can_revoke_operator(OperatorRole::Member, 1));
        assert!(can_revoke_operator(OperatorRole::Owner, 2));
        assert!(!can_revoke_operator(OperatorRole::Owner, 1));
    }
}
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::StorageError;

pub const BOOTSTRAP_OWNER_ID: Uuid = Uuid::from_u128(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperatorRole {
    Owner,
    Member,
}

impl OperatorRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Member => "member",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "owner" => Some(Self::Owner),
            "member" => Some(Self::Member),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OperatorRow {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub token_hash: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

pub fn hash_operator_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn hashes_equal(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}

pub fn can_revoke_operator(role: OperatorRole, active_owner_count: i64) -> bool {
    role != OperatorRole::Owner || active_owner_count > 1
}

fn generate_operator_token() -> String {
    format!("eh_{}_{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub async fn list_operators(pool: &PgPool) -> Result<Vec<OperatorRow>, StorageError> {
    Ok(sqlx::query_as::<_, OperatorRow>(
        "SELECT id, name, role, token_hash, active, created_at, updated_at, last_seen_at FROM operators ORDER BY created_at ASC, id ASC",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn find_operator_by_token_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<OperatorRow>, StorageError> {
    Ok(sqlx::query_as::<_, OperatorRow>(
        "SELECT id, name, role, token_hash, active, created_at, updated_at, last_seen_at FROM operators WHERE token_hash = $1 AND active = TRUE",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_operator(
    pool: &PgPool,
    operator_id: Uuid,
) -> Result<Option<OperatorRow>, StorageError> {
    Ok(sqlx::query_as::<_, OperatorRow>(
        "SELECT id, name, role, token_hash, active, created_at, updated_at, last_seen_at FROM operators WHERE id = $1",
    )
    .bind(operator_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn find_operator_by_name(
    pool: &PgPool,
    name: &str,
) -> Result<Option<OperatorRow>, StorageError> {
    Ok(sqlx::query_as::<_, OperatorRow>(
        "SELECT id, name, role, token_hash, active, created_at, updated_at, last_seen_at FROM operators WHERE name = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_operator(
    pool: &PgPool,
    name: &str,
    role: OperatorRole,
) -> Result<(OperatorRow, String), StorageError> {
    let token = generate_operator_token();
    let row = sqlx::query_as::<_, OperatorRow>(
        "INSERT INTO operators (name, role, token_hash) VALUES ($1, $2, $3) RETURNING id, name, role, token_hash, active, created_at, updated_at, last_seen_at",
    )
    .bind(name.trim())
    .bind(role.as_str())
    .bind(hash_operator_token(&token))
    .fetch_one(pool)
    .await?;
    Ok((row, token))
}

pub async fn rotate_operator_token(
    pool: &PgPool,
    operator_id: Uuid,
) -> Result<Option<(OperatorRow, String)>, StorageError> {
    let token = generate_operator_token();
    let row = sqlx::query_as::<_, OperatorRow>(
        "UPDATE operators SET token_hash = $2, active = TRUE, updated_at = now() WHERE id = $1 AND active = TRUE RETURNING id, name, role, token_hash, active, created_at, updated_at, last_seen_at",
    )
    .bind(operator_id)
    .bind(hash_operator_token(&token))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| (row, token)))
}

pub async fn active_owner_count(pool: &PgPool) -> Result<i64, StorageError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*)::bigint FROM operators WHERE role = 'owner' AND active = TRUE")
        .fetch_one(pool)
        .await?)
}

pub async fn revoke_operator(
    pool: &PgPool,
    operator_id: Uuid,
) -> Result<Option<OperatorRow>, StorageError> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query_as::<_, OperatorRow>(
        "SELECT id, name, role, token_hash, active, created_at, updated_at, last_seen_at FROM operators WHERE id = $1 FOR UPDATE",
    )
    .bind(operator_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let owners = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM operators WHERE role = 'owner' AND active = TRUE",
    )
    .fetch_one(&mut *transaction)
    .await?;
    let role = OperatorRole::parse(&row.role).ok_or_else(|| {
        StorageError::InvalidOperator("unknown operator role".into())
    })?;
    if !row.active || !can_revoke_operator(role, owners) {
        return Ok(None);
    }
    let updated = sqlx::query_as::<_, OperatorRow>(
        "UPDATE operators SET active = FALSE, updated_at = now() WHERE id = $1 RETURNING id, name, role, token_hash, active, created_at, updated_at, last_seen_at",
    )
    .bind(operator_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Some(updated))
}
