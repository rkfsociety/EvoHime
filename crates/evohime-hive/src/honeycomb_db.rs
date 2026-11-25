//! Dedicated SQLite storage for the Hive city state (Plan 200, Single-Writer).
//!
//! Only `evohime-hive` writes to this database; `evohime-core` may open it
//! strictly in read-only mode via [`HiveDatabase::open_read_only`] (`PRAGMA query_only = ON`).
//! Schema migrations run transactionally after creating a backup of the
//! existing database file.

use std::path::{Path, PathBuf};

use evohime_hive_contract::honeycomb::{
    HoneycombAgent, HoneycombAgentRole, HoneycombCity, HoneycombEdge, HoneycombHexCoord,
    HoneycombIntercityLink, HoneycombModuleNode, HoneycombMutationEvent, HoneycombSector,
    HoneycombVehicleDispatch, HoneycombWorldDelta, HoneycombWorldState, HONEYCOMB_SECTOR_CAPACITY,
    HONEYCOMB_WORLD_SCHEMA_VERSION,
};
use rusqlite::backup::Backup;
use rusqlite::{params, Connection, OpenFlags};
use thiserror::Error;

/// Current SQLite schema version of the Hive database.
pub const HIVE_DB_SCHEMA_VERSION: u32 = 1;

/// Maximum number of delta events stored in the replay window.
const MAX_STORED_EVENTS: i64 = 2_000;

/// Typed error returned by [`HiveDatabase`] operations.
#[derive(Debug, Error)]
pub enum HiveDbError {
    /// SQLite driver or query failure.
    #[error("hive sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem error while preparing the database directory or backup.
    #[error("hive db io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization/deserialization failure.
    #[error("hive db json error: {0}")]
    Json(#[from] serde_json::Error),
    /// Invalid or corrupted database record.
    #[error("hive db data error: {0}")]
    Corrupt(String),
}

/// Single-writer SQLite handle owned by `evohime-hive`.
pub struct HiveDatabase {
    conn: Connection,
}

impl HiveDatabase {
    /// Opens (or creates) the Hive SQLite database in read-write WAL mode and
    /// applies transactional migrations with a pre-migration backup.
    pub fn open(path: &Path) -> Result<Self, HiveDbError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;
        migrate_if_needed(&mut conn, Some(path))?;
        Ok(Self { conn })
    }

    /// Opens an in-memory Hive database (for isolated tests).
    pub fn open_in_memory() -> Result<Self, HiveDbError> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrate_if_needed(&mut conn, None)?;
        Ok(Self { conn })
    }

    /// Opens an existing Hive database strictly in read-only mode (`query_only = ON`)
    /// for inspection by `evohime-core` without violating single-writer ownership.
    pub fn open_read_only(path: &Path) -> Result<Self, HiveDbError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.execute_batch("PRAGMA query_only = ON;")?;
        Ok(Self { conn })
    }

    /// Loads the persisted world state across all registered project cities.
    pub fn load_world_state(
        &self,
        paused: bool,
        queen_working: bool,
        active_dispatches: Vec<HoneycombVehicleDispatch>,
    ) -> Result<HoneycombWorldState, HiveDbError> {
        let mut city_stmt = self.conn.prepare(
            "SELECT id, project_name, is_primary, has_queen_citadel, citadel_exits_count,
                    world_center_x, world_center_y, max_ring
             FROM hive_cities
             ORDER BY is_primary DESC, sort_order ASC, id ASC",
        )?;
        let city_rows = city_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, u8>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, u16>(7)?,
            ))
        })?;

        let mut cities = Vec::new();
        let mut primary_city_id = String::new();

        for row in city_rows {
            let (
                id,
                project_name,
                is_primary,
                has_queen_citadel,
                citadel_exits_count,
                world_center_x,
                world_center_y,
                max_ring,
            ) = row?;

            if is_primary && primary_city_id.is_empty() {
                primary_city_id = id.clone();
            }

            let sectors = self.load_sectors_for_city(&id)?;
            let modules = self.load_modules_for_city(&id)?;
            let edges = self.load_edges_for_city(&id)?;

            cities.push(HoneycombCity {
                id,
                project_name,
                is_primary,
                has_queen_citadel,
                citadel_exits_count,
                world_center_x,
                world_center_y,
                max_ring,
                sectors,
                modules,
                edges,
            });
        }

        if primary_city_id.is_empty() {
            if let Some(first) = cities.first() {
                primary_city_id = first.id.clone();
            }
        }

        let mut link_stmt = self.conn.prepare(
            "SELECT id, from_city_id, to_city_id, from_ring, to_ring
             FROM hive_intercity_links
             ORDER BY id ASC",
        )?;
        let intercity_links = link_stmt
            .query_map([], |row| {
                Ok(HoneycombIntercityLink {
                    id: row.get(0)?,
                    from_city_id: row.get(1)?,
                    to_city_id: row.get(2)?,
                    from_ring: row.get(3)?,
                    to_ring: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let revision = self.latest_revision()?;

        Ok(HoneycombWorldState {
            schema_version: HONEYCOMB_WORLD_SCHEMA_VERSION,
            paused,
            queen_working: !paused && queen_working,
            primary_city_id,
            cities,
            intercity_links,
            active_dispatches: if paused {
                Vec::new()
            } else {
                active_dispatches
            },
            revision,
        })
    }

    /// Persists the complete world state transactionally (cities, sectors, agents,
    /// modules, edges, intercity links, and current revision).
    pub fn save_world_state(&mut self, world: &HoneycombWorldState) -> Result<(), HiveDbError> {
        let tx = self.conn.transaction()?;

        tx.execute("DELETE FROM hive_edges", [])?;
        tx.execute("DELETE FROM hive_modules", [])?;
        tx.execute("DELETE FROM hive_agents", [])?;
        tx.execute("DELETE FROM hive_sectors", [])?;
        tx.execute("DELETE FROM hive_intercity_links", [])?;
        tx.execute("DELETE FROM hive_cities", [])?;

        for (idx, city) in world.cities.iter().enumerate() {
            tx.execute(
                "INSERT INTO hive_cities (
                    id, project_name, is_primary, has_queen_citadel, citadel_exits_count,
                    world_center_x, world_center_y, max_ring, sort_order
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    city.id,
                    city.project_name,
                    i64::from(city.is_primary),
                    i64::from(city.has_queen_citadel),
                    city.citadel_exits_count,
                    city.world_center_x,
                    city.world_center_y,
                    city.max_ring,
                    idx as i64,
                ],
            )?;

            for sector in &city.sectors {
                tx.execute(
                    "INSERT INTO hive_sectors (id, city_id, label, role, q, r)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        sector.id,
                        city.id,
                        sector.label,
                        role_to_str(sector.role),
                        sector.coord.q,
                        sector.coord.r,
                    ],
                )?;

                for agent in &sector.agents {
                    tx.execute(
                        "INSERT INTO hive_agents (
                            id, city_id, sector_id, name, role, topic, busy, active_trip_id
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            agent.id,
                            city.id,
                            sector.id,
                            agent.name,
                            role_to_str(agent.role),
                            agent.topic,
                            i64::from(agent.busy),
                            agent.active_trip_id,
                        ],
                    )?;
                }
            }

            for module in &city.modules {
                tx.execute(
                    "INSERT INTO hive_modules (
                        id, city_id, ring, q, r, name, kind, relative_path,
                        file_count, line_count, language
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        module.id,
                        city.id,
                        module.ring,
                        module.coord.q,
                        module.coord.r,
                        module.name,
                        module.kind,
                        module.relative_path,
                        module.file_count,
                        module.line_count as i64,
                        module.language,
                    ],
                )?;
            }

            for edge in &city.edges {
                tx.execute(
                    "INSERT INTO hive_edges (id, city_id, from_id, to_id, kind, active)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        edge.id,
                        city.id,
                        edge.from_id,
                        edge.to_id,
                        edge.kind,
                        i64::from(edge.active),
                    ],
                )?;
            }
        }

        for link in &world.intercity_links {
            tx.execute(
                "INSERT INTO hive_intercity_links (id, from_city_id, to_city_id, from_ring, to_ring)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    link.id,
                    link.from_city_id,
                    link.to_city_id,
                    link.from_ring,
                    link.to_ring,
                ],
            )?;
        }

        tx.execute(
            "INSERT OR REPLACE INTO hive_schema_meta (key, value) VALUES ('world_revision', ?1)",
            params![world.revision.to_string()],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Appends Queen mutation events for incremental pipe delta synchronization.
    pub fn append_mutation_events(
        &mut self,
        revision: u64,
        events: &[HoneycombMutationEvent],
        now_ms: i64,
    ) -> Result<(), HiveDbError> {
        if events.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        for event in events {
            let payload = serde_json::to_string(event)?;
            tx.execute(
                "INSERT INTO hive_events (revision, event_json, created_at_ms)
                 VALUES (?1, ?2, ?3)",
                params![revision as i64, payload, now_ms],
            )?;
        }
        tx.execute(
            "DELETE FROM hive_events
             WHERE seq NOT IN (
                SELECT seq FROM hive_events ORDER BY seq DESC LIMIT ?1
             )",
            params![MAX_STORED_EVENTS],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Reads incremental mutation events since `from_revision`.
    pub fn read_delta_since(
        &self,
        from_revision: u64,
        max_events: usize,
    ) -> Result<HoneycombWorldDelta, HiveDbError> {
        let latest = self.latest_revision()?;
        let min_stored_rev: Option<i64> = self
            .conn
            .query_row("SELECT MIN(revision) FROM hive_events", [], |row| {
                row.get(0)
            })
            .ok()
            .flatten();

        if let Some(min_rev) = min_stored_rev {
            if (from_revision as i64) < min_rev.saturating_sub(1) {
                return Ok(HoneycombWorldDelta {
                    schema_version: HONEYCOMB_WORLD_SCHEMA_VERSION,
                    from_revision,
                    to_revision: latest,
                    events: Vec::new(),
                    requires_full_resync: true,
                });
            }
        } else if from_revision < latest {
            return Ok(HoneycombWorldDelta {
                schema_version: HONEYCOMB_WORLD_SCHEMA_VERSION,
                from_revision,
                to_revision: latest,
                events: Vec::new(),
                requires_full_resync: true,
            });
        }

        let mut stmt = self.conn.prepare(
            "SELECT event_json FROM hive_events
             WHERE revision > ?1
             ORDER BY seq ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![from_revision as i64, max_events as i64], |row| {
            row.get::<_, String>(0)
        })?;

        let mut events = Vec::new();
        for row in rows {
            let json_str = row?;
            let event: HoneycombMutationEvent = serde_json::from_str(&json_str)?;
            events.push(event);
        }

        Ok(HoneycombWorldDelta {
            schema_version: HONEYCOMB_WORLD_SCHEMA_VERSION,
            from_revision,
            to_revision: latest,
            events,
            requires_full_resync: false,
        })
    }

    fn latest_revision(&self) -> Result<u64, HiveDbError> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM hive_schema_meta WHERE key = 'world_revision'",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(raw.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0))
    }

    fn load_sectors_for_city(&self, city_id: &str) -> Result<Vec<HoneycombSector>, HiveDbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, role, q, r
             FROM hive_sectors
             WHERE city_id = ?1
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([city_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i32>(4)?,
            ))
        })?;

        let mut sectors = Vec::new();
        for row in rows {
            let (id, label, role_raw, q, r) = row?;
            let role = str_to_role(&role_raw)?;
            let agents = self.load_agents_for_sector(&id)?;
            sectors.push(HoneycombSector {
                id,
                city_id: city_id.to_string(),
                coord: HoneycombHexCoord { q, r },
                ring: 1,
                label,
                role,
                capacity: HONEYCOMB_SECTOR_CAPACITY,
                agents,
            });
        }
        Ok(sectors)
    }

    fn load_agents_for_sector(&self, sector_id: &str) -> Result<Vec<HoneycombAgent>, HiveDbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, role, topic, busy, active_trip_id
             FROM hive_agents
             WHERE sector_id = ?1
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([sector_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;

        let mut agents = Vec::new();
        for row in rows {
            let (id, name, role_raw, topic, busy, active_trip_id) = row?;
            agents.push(HoneycombAgent {
                id,
                name,
                role: str_to_role(&role_raw)?,
                sector_id: sector_id.to_string(),
                topic,
                busy,
                active_trip_id,
            });
        }
        Ok(agents)
    }

    fn load_modules_for_city(
        &self,
        city_id: &str,
    ) -> Result<Vec<HoneycombModuleNode>, HiveDbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ring, q, r, name, kind, relative_path, file_count, line_count, language
             FROM hive_modules
             WHERE city_id = ?1
             ORDER BY ring ASC, id ASC",
        )?;
        let rows = stmt.query_map([city_id], |row| {
            Ok(HoneycombModuleNode {
                id: row.get(0)?,
                city_id: city_id.to_string(),
                ring: row.get(1)?,
                coord: HoneycombHexCoord {
                    q: row.get(2)?,
                    r: row.get(3)?,
                },
                name: row.get(4)?,
                kind: row.get(5)?,
                relative_path: row.get(6)?,
                file_count: row.get(7)?,
                line_count: row.get::<_, i64>(8)?.max(0) as u64,
                language: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(HiveDbError::from)
    }

    fn load_edges_for_city(&self, city_id: &str) -> Result<Vec<HoneycombEdge>, HiveDbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_id, to_id, kind, active
             FROM hive_edges
             WHERE city_id = ?1
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([city_id], |row| {
            Ok(HoneycombEdge {
                id: row.get(0)?,
                city_id: city_id.to_string(),
                from_id: row.get(1)?,
                to_id: row.get(2)?,
                kind: row.get(3)?,
                active: row.get::<_, i64>(4)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(HiveDbError::from)
    }
}

fn role_to_str(role: HoneycombAgentRole) -> &'static str {
    match role {
        HoneycombAgentRole::Executor => "executor",
        HoneycombAgentRole::Analyst => "analyst",
        HoneycombAgentRole::Strategist => "strategist",
        HoneycombAgentRole::DecisionCoordinator => "decision_coordinator",
    }
}

fn str_to_role(raw: &str) -> Result<HoneycombAgentRole, HiveDbError> {
    match raw {
        "executor" => Ok(HoneycombAgentRole::Executor),
        "analyst" => Ok(HoneycombAgentRole::Analyst),
        "strategist" => Ok(HoneycombAgentRole::Strategist),
        "decision_coordinator" => Ok(HoneycombAgentRole::DecisionCoordinator),
        other => Err(HiveDbError::Corrupt(format!("unknown agent role: {other}"))),
    }
}

fn backup_database_before_migration(src: &Connection, db_path: &Path) -> Result<(), HiveDbError> {
    let mut backup_os = db_path.as_os_str().to_os_string();
    backup_os.push(".pre-migrate.bak");
    let backup_path = PathBuf::from(backup_os);
    let mut dst = Connection::open(&backup_path)?;
    let backup = Backup::new(src, &mut dst)?;
    backup.run_to_completion(64, std::time::Duration::from_millis(10), None)?;
    Ok(())
}

fn migrate_if_needed(conn: &mut Connection, db_path: Option<&Path>) -> Result<(), HiveDbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS hive_schema_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;

    let current_ver: u32 = conn
        .query_row(
            "SELECT value FROM hive_schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    if current_ver >= HIVE_DB_SCHEMA_VERSION {
        return Ok(());
    }

    if current_ver > 0 {
        if let Some(path) = db_path {
            backup_database_before_migration(conn, path)?;
        }
    }

    let tx = conn.transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS hive_cities (
            id TEXT PRIMARY KEY,
            project_name TEXT NOT NULL,
            is_primary INTEGER NOT NULL,
            has_queen_citadel INTEGER NOT NULL,
            citadel_exits_count INTEGER NOT NULL,
            world_center_x REAL NOT NULL,
            world_center_y REAL NOT NULL,
            max_ring INTEGER NOT NULL,
            sort_order INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hive_sectors (
            id TEXT PRIMARY KEY,
            city_id TEXT NOT NULL REFERENCES hive_cities(id) ON DELETE CASCADE,
            label TEXT NOT NULL,
            role TEXT NOT NULL,
            q INTEGER NOT NULL,
            r INTEGER NOT NULL,
            UNIQUE(city_id, label)
        );
        CREATE TABLE IF NOT EXISTS hive_agents (
            id TEXT PRIMARY KEY,
            city_id TEXT NOT NULL REFERENCES hive_cities(id) ON DELETE CASCADE,
            sector_id TEXT NOT NULL REFERENCES hive_sectors(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            role TEXT NOT NULL,
            topic TEXT NOT NULL,
            busy INTEGER NOT NULL,
            active_trip_id TEXT
        );
        CREATE TABLE IF NOT EXISTS hive_modules (
            id TEXT PRIMARY KEY,
            city_id TEXT NOT NULL REFERENCES hive_cities(id) ON DELETE CASCADE,
            ring INTEGER NOT NULL,
            q INTEGER NOT NULL,
            r INTEGER NOT NULL,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            file_count INTEGER NOT NULL,
            line_count INTEGER NOT NULL,
            language TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hive_edges (
            id TEXT PRIMARY KEY,
            city_id TEXT NOT NULL REFERENCES hive_cities(id) ON DELETE CASCADE,
            from_id TEXT NOT NULL,
            to_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            active INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hive_intercity_links (
            id TEXT PRIMARY KEY,
            from_city_id TEXT NOT NULL,
            to_city_id TEXT NOT NULL,
            from_ring INTEGER NOT NULL,
            to_ring INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hive_events (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            revision INTEGER NOT NULL,
            event_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );",
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO hive_schema_meta (key, value) VALUES ('schema_version', ?1)",
        params![HIVE_DB_SCHEMA_VERSION.to_string()],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_world_state_and_enforces_read_only_for_core() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hive.db");

        let mut db = HiveDatabase::open(&db_path).expect("open rw");
        let world = HoneycombWorldState {
            schema_version: HONEYCOMB_WORLD_SCHEMA_VERSION,
            paused: false,
            queen_working: true,
            primary_city_id: "city-primary".to_string(),
            cities: vec![HoneycombCity {
                id: "city-primary".to_string(),
                project_name: "EvoHime".to_string(),
                is_primary: true,
                has_queen_citadel: true,
                citadel_exits_count: 2,
                world_center_x: 0.0,
                world_center_y: 0.0,
                max_ring: 1,
                sectors: vec![HoneycombSector {
                    id: "city-primary:sector:0".to_string(),
                    city_id: "city-primary".to_string(),
                    coord: HoneycombHexCoord { q: 0, r: 1 },
                    ring: 1,
                    label: "Исполнители".to_string(),
                    role: HoneycombAgentRole::Executor,
                    capacity: HONEYCOMB_SECTOR_CAPACITY,
                    agents: vec![HoneycombAgent {
                        id: "agent-1".to_string(),
                        name: "Агент #1".to_string(),
                        role: HoneycombAgentRole::Executor,
                        sector_id: "city-primary:sector:0".to_string(),
                        topic: "Индексация".to_string(),
                        busy: true,
                        active_trip_id: Some("trip-1".to_string()),
                    }],
                }],
                modules: vec![],
                edges: vec![],
            }],
            intercity_links: vec![],
            active_dispatches: vec![],
            revision: 7,
        };
        db.save_world_state(&world).expect("save world");

        // Core opens strictly read-only.
        let ro_db = HiveDatabase::open_read_only(&db_path).expect("open ro");
        let loaded = ro_db
            .load_world_state(true, false, vec![])
            .expect("load ro");
        assert_eq!(loaded.revision, 7);
        assert!(loaded.paused);
        assert!(loaded.active_dispatches.is_empty());
        assert_eq!(loaded.cities[0].sectors[0].label, "Исполнители");

        // Writing on read-only connection must fail.
        let write_attempt = ro_db.conn.execute("DELETE FROM hive_cities", []);
        assert!(write_attempt.is_err());
    }
}
