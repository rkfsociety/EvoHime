//! Migration-neutral bounded persistence contract for Memory v1.
//!
//! The module intentionally does not register itself in `lib.rs` or create a
//! migration. The storage owner decides when the compatible table exists;
//! this file keeps the record bounds and parameterized SQL contract together.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const MAX_ID_BYTES: usize = 256;
pub const MAX_SCOPE_ID_BYTES: usize = 512;
pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_CONTENT_BYTES: usize = 32 * 1024;
pub const MAX_PROVENANCE_BYTES: usize = 2 * 1024;
pub const MAX_TIMESTAMP_BYTES: usize = 64;
pub const MAX_QUERY_BYTES: usize = 512;
pub const MAX_TTL_SECONDS: u64 = 366 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Project,
    Task,
    Workspace,
}

impl MemoryScope {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Task => "task",
            Self::Workspace => "workspace",
        }
    }

    fn parse(value: &str) -> Result<Self, MemoryStoreError> {
        match value {
            "project" => Ok(Self::Project),
            "task" => Ok(Self::Task),
            "workspace" => Ok(Self::Workspace),
            _ => Err(MemoryStoreError::InvalidScope),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPrivacy {
    Public,
    Internal,
    Private,
}

impl MemoryPrivacy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Private => "private",
        }
    }

    fn parse(value: &str) -> Result<Self, MemoryStoreError> {
        match value {
            "public" => Ok(Self::Public),
            "internal" => Ok(Self::Internal),
            "private" => Ok(Self::Private),
            _ => Err(MemoryStoreError::InvalidPrivacy),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub title: String,
    pub content: String,
    pub provenance: String,
    pub privacy: MemoryPrivacy,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub archived: bool,
    pub forgotten: bool,
}

impl MemoryRecord {
    pub fn new(
        id: impl Into<String>,
        scope: MemoryScope,
        scope_id: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
        provenance: impl Into<String>,
        privacy: MemoryPrivacy,
        created_at: impl Into<String>,
        expires_at: Option<String>,
    ) -> Result<Self, MemoryStoreError> {
        let record = Self {
            id: id.into(),
            scope,
            scope_id: scope_id.into(),
            title: title.into(),
            content: redact_sensitive(&content.into()),
            provenance: provenance.into(),
            privacy,
            created_at: created_at.into(),
            expires_at,
            archived: false,
            forgotten: false,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), MemoryStoreError> {
        validate_required("id", &self.id, MAX_ID_BYTES)?;
        validate_required("scope_id", &self.scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("title", &self.title, MAX_TITLE_BYTES)?;
        validate_required("content", &self.content, MAX_CONTENT_BYTES)?;
        validate_required("provenance", &self.provenance, MAX_PROVENANCE_BYTES)?;
        validate_required("created_at", &self.created_at, MAX_TIMESTAMP_BYTES)?;
        if let Some(expires_at) = &self.expires_at {
            validate_required("expires_at", expires_at, MAX_TIMESTAMP_BYTES)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("invalid memory scope")]
    InvalidScope,
    #[error("invalid privacy label")]
    InvalidPrivacy,
    #[error("invalid TTL")]
    InvalidTtl,
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

fn validate_required(field: &'static str, value: &str, max: usize) -> Result<(), MemoryStoreError> {
    if value.trim().is_empty() {
        return Err(MemoryStoreError::Empty { field });
    }
    if value.len() > max {
        return Err(MemoryStoreError::Limit { field, max });
    }
    Ok(())
}

fn redact_sensitive(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if token.contains('@')
                || lower.starts_with("bearer")
                || lower.starts_with("sk-")
                || lower.starts_with("ghp_")
                || lower.starts_with("github_pat_")
                || lower.starts_with("api_key=")
                || lower.starts_with("token=")
            {
                "[REDACTED]".to_owned()
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parameterized SQL only; schema creation and migrations remain external.
pub struct MemoryStoreSql;

impl MemoryStoreSql {
    pub const INSERT: &'static str = "INSERT INTO memory_entries
        (id, scope_kind, scope_id, title, content, provenance, privacy,
         created_at, expires_at, archived, forgotten)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)";
    pub const SELECT_BY_ID: &'static str = "SELECT id, scope_kind, scope_id,
        title, content, provenance, privacy, created_at, expires_at,
        archived, forgotten FROM memory_entries WHERE id = ?1";
    pub const SEARCH: &'static str = "SELECT id, scope_kind, scope_id,
        title, content, provenance, privacy, created_at, expires_at,
        archived, forgotten FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND forgotten = 0 AND archived = 0
          AND (expires_at IS NULL OR expires_at > ?3)
          AND (lower(title) LIKE lower(?4) OR lower(content) LIKE lower(?4))
        ORDER BY id ASC LIMIT ?5";
    pub const ARCHIVE: &'static str =
        "UPDATE memory_entries SET archived = 1 WHERE id = ?1 AND forgotten = 0";
    pub const FORGET: &'static str =
        "UPDATE memory_entries SET content = '', provenance = '', forgotten = 1 WHERE id = ?1";

    pub fn insert(connection: &Connection, record: &MemoryRecord) -> Result<(), MemoryStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT,
            params![
                record.id,
                record.scope.as_str(),
                record.scope_id,
                record.title,
                record.content,
                record.provenance,
                record.privacy.as_str(),
                record.created_at,
                record.expires_at,
                record.archived as i64,
                record.forgotten as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<MemoryRecord>, MemoryStoreError> {
        Ok(connection
            .query_row(Self::SELECT_BY_ID, params![id], map_record)
            .optional()?)
    }

    pub fn search(
        connection: &Connection,
        scope: MemoryScope,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<MemoryRecord>, MemoryStoreError> {
        if query.len() > MAX_QUERY_BYTES {
            return Err(MemoryStoreError::Limit {
                field: "query",
                max: MAX_QUERY_BYTES,
            });
        }
        validate_required("scope_id", scope_id, MAX_SCOPE_ID_BYTES)?;
        validate_required("now", now, MAX_TIMESTAMP_BYTES)?;
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut statement = connection.prepare(Self::SEARCH)?;
        let records = statement
            .query_map(
                params![
                    scope.as_str(),
                    scope_id,
                    now,
                    pattern,
                    i64::from(limit.min(100))
                ],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn archive(connection: &Connection, id: &str) -> Result<bool, MemoryStoreError> {
        Ok(connection.execute(Self::ARCHIVE, params![id])? == 1)
    }

    pub fn forget(connection: &Connection, id: &str) -> Result<bool, MemoryStoreError> {
        Ok(connection.execute(Self::FORGET, params![id])? == 1)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let archived: i64 = row.get(9)?;
    let forgotten: i64 = row.get(10)?;
    Ok(MemoryRecord {
        id: row.get(0)?,
        scope: MemoryScope::parse(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        scope_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        provenance: row.get(5)?,
        privacy: MemoryPrivacy::parse(&row.get::<_, String>(6)?).map_err(to_sql_error)?,
        created_at: row.get(7)?,
        expires_at: row.get(8)?,
        archived: archived != 0,
        forgotten: forgotten != 0,
    })
}

fn to_sql_error(error: MemoryStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE memory_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    provenance TEXT NOT NULL,
                    privacy TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT,
                    archived INTEGER NOT NULL,
                    forgotten INTEGER NOT NULL
                );",
            )
            .expect("memory schema creates");
    }

    fn record(id: &str, content: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryScope::Project,
            "project-1",
            "Decision",
            content,
            "run:1",
            MemoryPrivacy::Internal,
            "2026-08-12T10:00:00Z",
            Some("2027-01-01T00:00:00Z".into()),
        )
        .expect("memory record builds")
    }

    #[test]
    fn constructor_redacts_sensitive_content_and_bounds_fields() {
        let memory = record("m-1", "contact roman@example.test with token=secret");
        assert!(!memory.content.contains("roman@example.test"));
        assert!(!memory.content.contains("token=secret"));

        let too_large = MemoryRecord::new(
            "m-1",
            MemoryScope::Project,
            "project-1",
            "title",
            "x".repeat(MAX_CONTENT_BYTES + 1),
            "run:1",
            MemoryPrivacy::Internal,
            "2026-08-12T10:00:00Z",
            None,
        );
        assert!(matches!(
            too_large,
            Err(MemoryStoreError::Limit {
                field: "content",
                ..
            })
        ));
    }

    #[test]
    fn round_trip_search_is_scoped_bounded_and_deterministic() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &record("b", "Rust decision")).expect("insert b");
        MemoryStoreSql::insert(&connection, &record("a", "Rust decision")).expect("insert a");
        let other = MemoryRecord {
            scope_id: "other-project".into(),
            ..record("c", "Rust decision")
        };
        MemoryStoreSql::insert(&connection, &other).expect("insert other");

        let found = MemoryStoreSql::search(
            &connection,
            MemoryScope::Project,
            "project-1",
            "rust",
            "2026-09-01T00:00:00Z",
            10,
        )
        .expect("search memories");
        assert_eq!(
            found
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            MemoryStoreSql::get_by_id(&connection, "a").unwrap(),
            Some(record("a", "Rust decision"))
        );
    }

    #[test]
    fn archive_and_forget_remove_memory_from_search_without_deleting_row() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        MemoryStoreSql::insert(&connection, &record("m-1", "keep this fact")).expect("insert");
        assert!(MemoryStoreSql::archive(&connection, "m-1").expect("archive"));
        assert!(MemoryStoreSql::search(
            &connection,
            MemoryScope::Project,
            "project-1",
            "fact",
            "2026-09-01T00:00:00Z",
            10
        )
        .unwrap()
        .is_empty());

        assert!(MemoryStoreSql::forget(&connection, "m-1").expect("forget"));
        let forgotten = MemoryStoreSql::get_by_id(&connection, "m-1")
            .unwrap()
            .unwrap();
        assert!(forgotten.forgotten);
        assert!(forgotten.content.is_empty());
        assert!(forgotten.provenance.is_empty());
    }
}
