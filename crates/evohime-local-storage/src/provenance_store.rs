use rusqlite::OptionalExtension;

use crate::{ImportedTask, LocalDatabase, ProvenanceRecord, StorageError, WorkItemRecord};

impl LocalDatabase {
    /// Imports a PRD provenance record and its derived backlog work items.
    ///
    /// The provenance payload retains the source text and version. The record
    /// and all imported tasks are inserted in one transaction, so a failed task
    /// insert does not leave a partial import.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] for serialization, database, or post-insert
    /// lookup failures.
    pub fn import_prd(
        &self,
        provenance_id: &str,
        project_id: &str,
        origin: &str,
        version: &str,
        source_text: &str,
        tasks: &[ImportedTask],
    ) -> Result<Vec<WorkItemRecord>, StorageError> {
        let payload = serde_json::to_vec(&serde_json::json!({
            "version": version,
            "source_text": source_text,
        }))?;
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO provenance(id, kind, source, payload) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![provenance_id, "prd_import", origin, payload],
        )?;
        for task in tasks {
            transaction.execute(
                "INSERT INTO work_items(id, project_id, title, description, source_ref,
                 acceptance_criteria, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'backlog')",
                rusqlite::params![
                    task.id,
                    project_id,
                    task.title,
                    task.description,
                    task.source_ref,
                    task.acceptance_criteria,
                ],
            )?;
        }
        transaction.commit()?;
        tasks
            .iter()
            .map(|task| {
                self.get_work_item(&task.id)?
                    .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
            })
            .collect()
    }

    /// Returns a provenance record by ID, or `None` when it is absent.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the SQLite query fails.
    pub fn get_provenance(&self, id: &str) -> Result<Option<ProvenanceRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, kind, source, payload FROM provenance WHERE id = ?1",
                [id],
                |row| {
                    Ok(ProvenanceRecord {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        source: row.get(2)?,
                        payload: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }
}
