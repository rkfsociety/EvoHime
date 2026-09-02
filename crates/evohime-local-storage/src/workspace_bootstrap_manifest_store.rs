//! Durable metadata for Workspace Bootstrap Manifest preparation.
use rusqlite::{params, Connection, OptionalExtension};

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS workspace_bootstrap_manifests (manifest_id TEXT NOT NULL, workspace_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, manifest_json TEXT NOT NULL, trust_status TEXT NOT NULL, policy_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY (manifest_id, revision)); CREATE TABLE IF NOT EXISTS workspace_bootstrap_preparations (workspace_id TEXT NOT NULL, manifest_id TEXT NOT NULL, manifest_hash TEXT NOT NULL, fingerprint TEXT NOT NULL, status TEXT NOT NULL, lease_id TEXT, version INTEGER NOT NULL, result_json TEXT, updated_at_ms INTEGER NOT NULL, PRIMARY KEY (workspace_id, manifest_id, manifest_hash, fingerprint)); CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_bootstrap_running ON workspace_bootstrap_preparations(workspace_id, manifest_id) WHERE status='running';")
}

pub fn put_manifest(
    c: &Connection,
    row: (&str, &str, u64, &str, &str, &str, i64),
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO workspace_bootstrap_manifests (manifest_id,workspace_id,revision,content_hash,manifest_json,trust_status,policy_hash,updated_at_ms) VALUES (?1,?2,?3,?4,?5,'pending_review',?6,?7)", params![row.0,row.1,row.2 as i64,row.3,row.4,row.5,row.6])? == 1)
}

pub fn get_preparation(
    c: &Connection,
    workspace_id: &str,
    manifest_id: &str,
    manifest_hash: &str,
    fingerprint: &str,
) -> rusqlite::Result<Option<(String, String, i64)>> {
    c.query_row("SELECT fingerprint,status,version FROM workspace_bootstrap_preparations WHERE workspace_id=?1 AND manifest_id=?2 AND manifest_hash=?3 AND fingerprint=?4", params![workspace_id, manifest_id, manifest_hash, fingerprint], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()
}

pub fn fence_expired_preparations(c: &Connection, cutoff_ms: i64) -> rusqlite::Result<usize> {
    c.execute("UPDATE workspace_bootstrap_preparations SET status='unknown_outcome',lease_id=NULL,version=version+1,updated_at_ms=?1 WHERE status='running' AND updated_at_ms<?1", params![cutoff_ms])
}

pub fn reserve_preparation(
    c: &Connection,
    workspace_id: &str,
    manifest_id: &str,
    manifest_hash: &str,
    fingerprint: &str,
    lease_id: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO workspace_bootstrap_preparations (workspace_id,manifest_id,manifest_hash,fingerprint,status,lease_id,version,updated_at_ms) VALUES (?1,?2,?3,?4,'running',?5,1,?6)", params![workspace_id,manifest_id,manifest_hash,fingerprint,lease_id,now_ms])? == 1)
}

pub fn complete_preparation(
    c: &Connection,
    workspace_id: &str,
    manifest_id: &str,
    lease_id: &str,
    status: &str,
    result_json: Option<&str>,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("UPDATE workspace_bootstrap_preparations SET status=?1,lease_id=NULL,result_json=?2,version=version+1,updated_at_ms=?3 WHERE workspace_id=?4 AND manifest_id=?5 AND lease_id=?6 AND status='running'", params![status,result_json,now_ms,workspace_id,manifest_id,lease_id])? == 1)
}

pub fn approve_manifest(
    c: &Connection,
    manifest_id: &str,
    revision: u64,
    content_hash: &str,
    policy_hash: &str,
) -> rusqlite::Result<bool> {
    Ok(c.execute("UPDATE workspace_bootstrap_manifests SET trust_status='trusted',policy_hash=?1 WHERE manifest_id=?2 AND revision=?3 AND content_hash=?4 AND trust_status='pending_review'", params![policy_hash,manifest_id,revision as i64,content_hash])? == 1)
}

pub fn manifest_trust(
    c: &Connection,
    manifest_id: &str,
    revision: u64,
) -> rusqlite::Result<Option<(String, String)>> {
    c.query_row("SELECT trust_status,content_hash FROM workspace_bootstrap_manifests WHERE manifest_id=?1 AND revision=?2", params![manifest_id,revision as i64], |r| Ok((r.get(0)?,r.get(1)?))).optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_and_preparation_are_durable_and_single_flight() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_manifest(&c, ("m", "w", 1, "h", "{}", "p", 1)).unwrap());
        assert!(reserve_preparation(&c, "w", "m", "h", "f", "l1", 2).unwrap());
        assert!(!reserve_preparation(&c, "w", "m", "h", "f", "l2", 3).unwrap());
        assert_eq!(
            get_preparation(&c, "w", "m", "h", "f").unwrap().unwrap(),
            ("f".into(), "running".into(), 1)
        );
        assert!(fence_expired_preparations(&c, 3).unwrap() == 1);
        assert!(reserve_preparation(&c, "w", "m", "h2", "f2", "l2", 4).unwrap());
        assert!(approve_manifest(&c, "m", 1, "h", "policy").unwrap());
        assert_eq!(manifest_trust(&c, "m", 1).unwrap().unwrap().0, "trusted");
    }
}
