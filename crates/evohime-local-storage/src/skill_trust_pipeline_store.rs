//! Durable metadata-only store for Skill Trust decisions.

use rusqlite::{params, Connection, OptionalExtension};

pub const STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillTrustRecord {
    pub skill_id: String,
    pub content_hash: String,
    pub scanner_version: String,
    pub review_policy_version: String,
    pub decision: String,
    pub risk_class: String,
    pub findings_json: String,
    pub override_actor: Option<String>,
    pub revision: i64,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS skill_trust_records (
        skill_id TEXT NOT NULL, content_hash TEXT NOT NULL, scanner_version TEXT NOT NULL,
        review_policy_version TEXT NOT NULL, decision TEXT NOT NULL, risk_class TEXT NOT NULL,
        findings_json TEXT NOT NULL, override_actor TEXT, revision INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        PRIMARY KEY(skill_id, content_hash, scanner_version, review_policy_version)
    ); CREATE TABLE IF NOT EXISTS skill_trust_audit (
        audit_id INTEGER PRIMARY KEY AUTOINCREMENT, skill_id TEXT NOT NULL, content_hash TEXT NOT NULL,
        actor TEXT NOT NULL, action TEXT NOT NULL, revision INTEGER NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );")
}

pub fn upsert(connection: &Connection, record: &SkillTrustRecord) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO skill_trust_records(skill_id,content_hash,scanner_version,review_policy_version,decision,risk_class,findings_json,override_actor,revision) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(skill_id,content_hash,scanner_version,review_policy_version) DO UPDATE SET decision=excluded.decision,risk_class=excluded.risk_class,findings_json=excluded.findings_json,override_actor=excluded.override_actor,revision=excluded.revision,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![record.skill_id,record.content_hash,record.scanner_version,record.review_policy_version,record.decision,record.risk_class,record.findings_json,record.override_actor,record.revision])?;
    Ok(())
}

pub fn get(
    connection: &Connection,
    skill_id: &str,
    content_hash: &str,
    scanner_version: &str,
    review_policy_version: &str,
) -> rusqlite::Result<Option<SkillTrustRecord>> {
    connection.query_row("SELECT skill_id,content_hash,scanner_version,review_policy_version,decision,risk_class,findings_json,override_actor,revision FROM skill_trust_records WHERE skill_id=?1 AND content_hash=?2 AND scanner_version=?3 AND review_policy_version=?4", params![skill_id,content_hash,scanner_version,review_policy_version], |row| Ok(SkillTrustRecord { skill_id:row.get(0)?,content_hash:row.get(1)?,scanner_version:row.get(2)?,review_policy_version:row.get(3)?,decision:row.get(4)?,risk_class:row.get(5)?,findings_json:row.get(6)?,override_actor:row.get(7)?,revision:row.get(8)? })).optional()
}

pub fn audit(
    connection: &Connection,
    skill_id: &str,
    content_hash: &str,
    actor: &str,
    action: &str,
    revision: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO skill_trust_audit(skill_id,content_hash,actor,action,revision) VALUES (?1,?2,?3,?4,?5)", params![skill_id, content_hash, actor, action, revision])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_round_trip_does_not_store_body() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let r = SkillTrustRecord {
            skill_id: "x".into(),
            content_hash: "h".into(),
            scanner_version: "v".into(),
            review_policy_version: "p".into(),
            decision: "trusted".into(),
            risk_class: "low".into(),
            findings_json: "[]".into(),
            override_actor: None,
            revision: 1,
        };
        upsert(&c, &r).unwrap();
        assert_eq!(get(&c, "x", "h", "v", "p").unwrap(), Some(r));
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='skill_trust_records'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
