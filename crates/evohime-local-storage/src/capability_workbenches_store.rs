//! SQLite persistence for capability workbench instances, snapshots, and renewable leases.

use rusqlite::{params, Connection, OptionalExtension};

/// Schema version for the capability workbench tables.
pub const STORE_SCHEMA_VERSION: u32 = 1;

/// Creates the workbench instance, snapshot, lease tables, and lease-expiry index.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS capability_workbench_instances (
           instance_id TEXT PRIMARY KEY NOT NULL,
           owner_id TEXT NOT NULL,
           revision INTEGER NOT NULL,
           lifecycle TEXT NOT NULL,
           descriptor_json BLOB NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS capability_workbench_snapshots (
           snapshot_id TEXT PRIMARY KEY NOT NULL,
           instance_id TEXT NOT NULL,
           revision INTEGER NOT NULL,
           snapshot_json BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL,
           FOREIGN KEY(instance_id) REFERENCES capability_workbench_instances(instance_id)
         );
         CREATE TABLE IF NOT EXISTS capability_workbench_leases (
           lease_id TEXT PRIMARY KEY NOT NULL,
           instance_id TEXT NOT NULL,
           owner_id TEXT NOT NULL,
           state TEXT NOT NULL,
           expires_at_ms INTEGER NOT NULL,
           heartbeat_at_ms INTEGER NOT NULL,
           FOREIGN KEY(instance_id) REFERENCES capability_workbench_instances(instance_id)
         );
         CREATE INDEX IF NOT EXISTS idx_capability_workbench_leases_expiry
           ON capability_workbench_leases(state, expires_at_ms);",
    )
}

/// Inserts a workbench instance with its owner, revision, lifecycle, and serialized descriptor.
pub fn put_instance(
    connection: &Connection,
    instance_id: &str,
    owner_id: &str,
    revision: i64,
    lifecycle: &str,
    descriptor_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO capability_workbench_instances(instance_id,owner_id,revision,lifecycle,descriptor_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6)", params![instance_id, owner_id, revision, lifecycle, descriptor_json, now_ms])?;
    Ok(())
}

/// Loads the serialized descriptor for an instance, returning `None` if it is absent.
pub fn get_instance(
    connection: &Connection,
    instance_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT descriptor_json FROM capability_workbench_instances WHERE instance_id=?1",
            [instance_id],
            |row| row.get(0),
        )
        .optional()
}

/// Replaces an instance only when its current revision equals `expected_revision`.
///
/// Returns `false` when another writer has advanced the instance or the identifier is unknown.
pub fn replace_instance(
    connection: &Connection,
    instance_id: &str,
    expected_revision: i64,
    revision: i64,
    lifecycle: &str,
    descriptor_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(connection.execute("UPDATE capability_workbench_instances SET revision=?1,lifecycle=?2,descriptor_json=?3,updated_at_ms=?4 WHERE instance_id=?5 AND revision=?6", params![revision, lifecycle, descriptor_json, now_ms, instance_id, expected_revision])? == 1)
}

/// Stores an immutable serialized snapshot associated with an instance revision.
pub fn put_snapshot(
    connection: &Connection,
    snapshot_id: &str,
    instance_id: &str,
    revision: i64,
    snapshot_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO capability_workbench_snapshots(snapshot_id,instance_id,revision,snapshot_json,created_at_ms) VALUES (?1,?2,?3,?4,?5)", params![snapshot_id, instance_id, revision, snapshot_json, now_ms])?;
    Ok(())
}

/// Creates an active lease owned by `owner_id` until `expires_at_ms`.
pub fn put_lease(
    connection: &Connection,
    lease_id: &str,
    instance_id: &str,
    owner_id: &str,
    expires_at_ms: i64,
    heartbeat_at_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO capability_workbench_leases(lease_id,instance_id,owner_id,state,expires_at_ms,heartbeat_at_ms) VALUES (?1,?2,?3,'active',?4,?5)", params![lease_id, instance_id, owner_id, expires_at_ms, heartbeat_at_ms])?;
    Ok(())
}

/// Renews the named lease when it belongs to `owner_id`.
///
/// Returns `false` when the lease does not exist or has a different owner.
pub fn renew_lease(
    connection: &Connection,
    lease_id: &str,
    owner_id: &str,
    heartbeat_at_ms: i64,
    expires_at_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(connection.execute(
        "UPDATE capability_workbench_leases SET state='active',heartbeat_at_ms=?1,expires_at_ms=?2 WHERE lease_id=?3 AND owner_id=?4",
        params![heartbeat_at_ms, expires_at_ms, lease_id, owner_id],
    )? == 1)
}

/// Marks active leases expired when their expiry time is earlier than `now_ms`.
///
/// Returns the number of leases changed.
pub fn expire_leases(connection: &Connection, now_ms: i64) -> rusqlite::Result<usize> {
    connection.execute(
        "UPDATE capability_workbench_leases SET state='expired' WHERE state='active' AND expires_at_ms < ?1",
        [now_ms],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_and_optimistic_instance_update_are_atomic_contract() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        put_instance(&connection, "i", "o", 1, "created", b"{}", 1).unwrap();
        assert!(!replace_instance(&connection, "i", 0, 2, "ready", b"{}", 2).unwrap());
        assert!(replace_instance(&connection, "i", 1, 2, "ready", b"{}", 2).unwrap());
        assert_eq!(
            get_instance(&connection, "i").unwrap(),
            Some(b"{}".to_vec())
        );
    }
}
