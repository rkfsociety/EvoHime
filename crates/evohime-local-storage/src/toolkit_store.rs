use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 256;
pub const MAX_VERSIONS_PER_TOOLKIT: usize = 32;
pub const MAX_METADATA_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolkitRecord {
    pub toolkit_id: String,
    pub version: String,
    pub manifest_hash: String,
    pub source: String,
    pub package_hash: Option<String>,
    pub license: Option<String>,
    pub status: String,
    pub compatible_core: String,
    pub manifest_json: Vec<u8>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolkitAuditRecord {
    pub toolkit_id: String,
    pub version: String,
    pub from_status: Option<String>,
    pub to_status: String,
    pub reason: String,
    pub created_at: String,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS toolkit_versions (
            toolkit_id TEXT NOT NULL,
            version TEXT NOT NULL,
            manifest_hash TEXT NOT NULL,
            source TEXT NOT NULL,
            package_hash TEXT,
            license TEXT,
            status TEXT NOT NULL CHECK(status IN ('available','enabled','disabled','quarantined','unavailable')),
            compatible_core TEXT NOT NULL,
            manifest_json BLOB NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            PRIMARY KEY(toolkit_id, version)
        );
        CREATE INDEX IF NOT EXISTS idx_toolkit_versions_status ON toolkit_versions(status, toolkit_id);
        CREATE TABLE IF NOT EXISTS toolkit_audit (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            toolkit_id TEXT NOT NULL,
            version TEXT NOT NULL,
            from_status TEXT,
            to_status TEXT NOT NULL,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );",
    )
}

fn validate_text(name: &str, value: &str, max: usize) -> rusqlite::Result<()> {
    if value.trim().is_empty() || value.len() > max {
        return Err(rusqlite::Error::InvalidParameterName(name.to_owned()));
    }
    Ok(())
}

pub fn discover(connection: &Connection, record: &ToolkitRecord) -> rusqlite::Result<()> {
    validate_text("toolkit_id", &record.toolkit_id, 256)?;
    validate_text("version", &record.version, 64)?;
    validate_text("manifest_hash", &record.manifest_hash, 256)?;
    if record.manifest_json.len() > MAX_METADATA_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "metadata too large").into(),
        ));
    }
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM toolkit_versions", [], |row| {
        row.get(0)
    })?;
    let existing: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM toolkit_versions WHERE toolkit_id=?1 AND version=?2",
            params![record.toolkit_id, record.version],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_none() && count as usize >= MAX_ENTRIES {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "catalog limit exceeded").into(),
        ));
    }
    let versions: i64 = connection.query_row(
        "SELECT COUNT(*) FROM toolkit_versions WHERE toolkit_id=?1",
        [&record.toolkit_id],
        |row| row.get(0),
    )?;
    if existing.is_none() && versions as usize >= MAX_VERSIONS_PER_TOOLKIT {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "version limit exceeded").into(),
        ));
    }
    connection.execute("INSERT INTO toolkit_versions(toolkit_id,version,manifest_hash,source,package_hash,license,status,compatible_core,manifest_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(toolkit_id,version) DO UPDATE SET manifest_hash=excluded.manifest_hash, source=excluded.source, package_hash=excluded.package_hash, license=excluded.license, compatible_core=excluded.compatible_core, manifest_json=excluded.manifest_json, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![record.toolkit_id,record.version,record.manifest_hash,record.source,record.package_hash,record.license,record.status,record.compatible_core,record.manifest_json])?;
    Ok(())
}

pub fn list(connection: &Connection, limit: usize) -> rusqlite::Result<Vec<ToolkitRecord>> {
    let mut statement = connection.prepare("SELECT toolkit_id,version,manifest_hash,source,package_hash,license,status,compatible_core,manifest_json,created_at,updated_at FROM toolkit_versions ORDER BY toolkit_id,version LIMIT ?1")?;
    statement
        .query_map([limit.min(MAX_ENTRIES) as i64], |row| {
            Ok(ToolkitRecord {
                toolkit_id: row.get(0)?,
                version: row.get(1)?,
                manifest_hash: row.get(2)?,
                source: row.get(3)?,
                package_hash: row.get(4)?,
                license: row.get(5)?,
                status: row.get(6)?,
                compatible_core: row.get(7)?,
                manifest_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .and_then(|rows| rows.collect())
}

pub fn transition(
    connection: &Connection,
    toolkit_id: &str,
    version: &str,
    status: &str,
    reason: &str,
) -> rusqlite::Result<()> {
    if !matches!(
        status,
        "available" | "enabled" | "disabled" | "quarantined" | "unavailable"
    ) {
        return Err(rusqlite::Error::InvalidParameterName("status".into()));
    }
    let tx = connection.unchecked_transaction()?;
    let old: Option<String> = tx
        .query_row(
            "SELECT status FROM toolkit_versions WHERE toolkit_id=?1 AND version=?2",
            params![toolkit_id, version],
            |row| row.get(0),
        )
        .optional()?;
    let Some(old_status) = old else {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    };
    if old_status == "quarantined" && status == "enabled" {
        return Err(rusqlite::Error::InvalidParameterName(
            "quarantined toolkit requires explicit restore".into(),
        ));
    }
    tx.execute("UPDATE toolkit_versions SET status=?3,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE toolkit_id=?1 AND version=?2", params![toolkit_id,version,status])?;
    let bounded_reason: String = reason.chars().take(512).collect();
    tx.execute("INSERT INTO toolkit_audit(toolkit_id,version,from_status,to_status,reason) VALUES (?1,?2,?3,?4,?5)", params![toolkit_id,version,old_status,status,bounded_reason])?;
    tx.commit()
}

/// Atomically selects one version and disables every other enabled version.
/// This makes rollback a durable catalog operation rather than an alias for
/// enabling a second version alongside the active one.
pub fn rollback(
    connection: &Connection,
    toolkit_id: &str,
    version: &str,
    reason: &str,
) -> rusqlite::Result<()> {
    let tx = connection.unchecked_transaction()?;
    let target: String = tx.query_row(
        "SELECT status FROM toolkit_versions WHERE toolkit_id=?1 AND version=?2",
        params![toolkit_id, version],
        |row| row.get(0),
    )?;
    if target == "quarantined" || target == "unavailable" {
        return Err(rusqlite::Error::InvalidParameterName(
            "target is not executable".into(),
        ));
    }
    let bounded_reason: String = reason.chars().take(512).collect();
    let mut enabled = tx.prepare(
        "SELECT version,status FROM toolkit_versions WHERE toolkit_id=?1 AND status='enabled'",
    )?;
    let current: Vec<(String, String)> = enabled
        .query_map(params![toolkit_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(enabled);
    for (old_version, old_status) in current {
        tx.execute(
            "UPDATE toolkit_versions SET status='disabled',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE toolkit_id=?1 AND version=?2",
            params![toolkit_id, old_version],
        )?;
        tx.execute(
            "INSERT INTO toolkit_audit(toolkit_id,version,from_status,to_status,reason) VALUES (?1,?2,?3,'disabled',?4)",
            params![toolkit_id, old_version, old_status, bounded_reason],
        )?;
    }
    tx.execute(
        "UPDATE toolkit_versions SET status='enabled',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE toolkit_id=?1 AND version=?2",
        params![toolkit_id, version],
    )?;
    tx.execute(
        "INSERT INTO toolkit_audit(toolkit_id,version,from_status,to_status,reason) VALUES (?1,?2,?3,'enabled',?4)",
        params![toolkit_id, version, target, bounded_reason],
    )?;
    tx.commit()
}

pub fn audit(
    connection: &Connection,
    toolkit_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<ToolkitAuditRecord>> {
    let mut s=connection.prepare("SELECT toolkit_id,version,from_status,to_status,reason,created_at FROM toolkit_audit WHERE toolkit_id=?1 ORDER BY id DESC LIMIT ?2")?;
    s.query_map(params![toolkit_id, limit.min(1024) as i64], |row| {
        Ok(ToolkitAuditRecord {
            toolkit_id: row.get(0)?,
            version: row.get(1)?,
            from_status: row.get(2)?,
            to_status: row.get(3)?,
            reason: row.get(4)?,
            created_at: row.get(5)?,
        })
    })
    .and_then(|rows| rows.collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(version: &str) -> ToolkitRecord {
        ToolkitRecord {
            toolkit_id: "builtin.filesystem".into(),
            version: version.into(),
            manifest_hash: format!("sha256:{version}"),
            source: "builtin".into(),
            package_hash: None,
            license: Some("MIT".into()),
            status: "available".into(),
            compatible_core: ">=0.1".into(),
            manifest_json: br#"{"kind":"tool/manifest/v1"}"#.to_vec(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn lifecycle_survives_reopen_and_records_rollback_history() {
        let path =
            std::env::temp_dir().join(format!("evohime-toolkit-store-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let connection = Connection::open(&path).unwrap();
        install_schema(&connection).unwrap();
        discover(&connection, &record("1.0.0")).unwrap();
        discover(&connection, &record("2.0.0")).unwrap();
        transition(
            &connection,
            "builtin.filesystem",
            "1.0.0",
            "enabled",
            "initial enable",
        )
        .unwrap();
        transition(
            &connection,
            "builtin.filesystem",
            "1.0.0",
            "disabled",
            "rollback",
        )
        .unwrap();
        assert_eq!(list(&connection, 10).unwrap().len(), 2);
        assert_eq!(
            audit(&connection, "builtin.filesystem", 10).unwrap().len(),
            2
        );
        drop(connection);
        let reopened = Connection::open(&path).unwrap();
        assert_eq!(
            list(&reopened, 10)
                .unwrap()
                .iter()
                .find(|r| r.version == "1.0.0")
                .unwrap()
                .status,
            "disabled"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn quarantine_cannot_become_enabled_implicitly() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        discover(&connection, &record("1.0.0")).unwrap();
        transition(
            &connection,
            "builtin.filesystem",
            "1.0.0",
            "quarantined",
            "hash mismatch",
        )
        .unwrap();
        assert!(transition(
            &connection,
            "builtin.filesystem",
            "1.0.0",
            "enabled",
            "enable"
        )
        .is_err());
    }
}
