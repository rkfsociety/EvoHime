use rusqlite::OptionalExtension;

use crate::{LocalDatabase, ProjectPolicyRecord, ProjectRecord, StorageError};

impl LocalDatabase {
    pub fn create_project(
        &self,
        id: &str,
        title: &str,
        workspace_path: &str,
        source_ref: Option<&str>,
    ) -> Result<ProjectRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO projects(id, title, workspace_path, source_ref) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title,
             workspace_path = excluded.workspace_path, source_ref = excluded.source_ref",
            rusqlite::params![id, title, workspace_path, source_ref],
        )?;
        self.get_project(id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, workspace_path, source_ref, version FROM projects WHERE id = ?1",
        )?;
        Ok(statement
            .query_row([id], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    workspace_path: row.get(2)?,
                    source_ref: row.get(3)?,
                    version: row.get(4)?,
                })
            })
            .optional()?)
    }

    pub fn get_project_by_workspace_path(
        &self,
        workspace_path: &str,
    ) -> Result<Option<ProjectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, workspace_path, source_ref, version FROM projects WHERE workspace_path = ?1 LIMIT 1",
        )?;
        Ok(statement
            .query_row([workspace_path], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    workspace_path: row.get(2)?,
                    source_ref: row.get(3)?,
                    version: row.get(4)?,
                })
            })
            .optional()?)
    }

    pub fn get_project_policy(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectPolicyRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT project_id, policy_json, version, updated_at FROM project_policies WHERE project_id = ?1",
                [project_id],
                |row| {
                    Ok(ProjectPolicyRecord {
                        project_id: row.get(0)?,
                        policy_json: row.get(1)?,
                        version: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn upsert_project_policy(
        &self,
        project_id: &str,
        policy_json: &[u8],
        expected_version: Option<i64>,
    ) -> Result<ProjectPolicyRecord, StorageError> {
        let current = self.get_project_policy(project_id)?;
        match (current, expected_version) {
            (Some(record), Some(expected)) if record.version != expected => {
                return Err(StorageError::VersionConflict {
                    entity: "project_policy",
                    id: project_id.into(),
                    expected,
                    current: record.version,
                });
            }
            _ => {}
        }
        self.connection.execute(
            "INSERT INTO project_policies(project_id, policy_json, version) VALUES (?1, ?2, 1)
             ON CONFLICT(project_id) DO UPDATE SET policy_json = excluded.policy_json, version = project_policies.version + 1,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            rusqlite::params![project_id, policy_json],
        )?;
        self.get_project_policy(project_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }
}
