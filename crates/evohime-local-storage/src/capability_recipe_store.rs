//! Metadata-only linkage between a guided recipe and its existing workflow run.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

/// Immutable recipe attribution stored beside an existing workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeRunLink {
    /// Existing workflow run id; this is the lifecycle and recovery authority.
    pub run_id: String,
    /// Stable built-in recipe id.
    pub recipe_id: String,
    /// Immutable recipe definition version.
    pub recipe_version: u32,
    /// Canonical recipe descriptor digest.
    pub recipe_hash: String,
    /// Exact existing workflow template id.
    pub template_id: String,
    /// Exact workflow template version.
    pub template_version: u32,
    /// Digest of the uninstantiated template graph.
    pub template_graph_hash: String,
    /// Digest of the instantiated graph stored on the workflow run.
    pub run_graph_hash: String,
    /// Digest of bounded workflow inputs; raw input remains owned by workflow storage.
    pub input_hash: String,
    /// Digest of the selected workspace; the raw path remains owned by the workflow run.
    pub workspace_hash: String,
    /// Idempotency key scoped to recipe id and version.
    pub idempotency_key: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: i64,
}

/// Creates the recipe-link table after `workflow_runs` has been installed.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS capability_recipe_run_links (
            run_id TEXT PRIMARY KEY NOT NULL
                REFERENCES workflow_runs(run_id) ON DELETE CASCADE,
            recipe_id TEXT NOT NULL,
            recipe_version INTEGER NOT NULL CHECK(recipe_version > 0),
            recipe_hash TEXT NOT NULL,
            template_id TEXT NOT NULL,
            template_version INTEGER NOT NULL CHECK(template_version > 0),
            template_graph_hash TEXT NOT NULL,
            run_graph_hash TEXT NOT NULL,
            input_hash TEXT NOT NULL,
            workspace_hash TEXT NOT NULL,
            idempotency_key TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            UNIQUE(recipe_id, recipe_version, idempotency_key)
        );
        CREATE INDEX IF NOT EXISTS idx_capability_recipe_run_links_recipe
            ON capability_recipe_run_links(recipe_id, recipe_version, created_at_ms);",
    )
}

/// Inserts a validated recipe link into the caller's workflow transaction.
///
/// Repeating the same recipe/version/idempotency key and content is a no-op;
/// using that key for different metadata fails without replacing history.
///
/// # Errors
///
/// Returns a SQLite error for invalid bounds, conflicting idempotency reuse,
/// or database failures.
pub fn insert_link(
    transaction: &Transaction<'_>,
    link: &RecipeRunLink,
) -> rusqlite::Result<()> {
    validate(link)?;
    let existing = transaction
        .query_row(
            "SELECT run_id, recipe_hash, template_id, template_version,
                    template_graph_hash, run_graph_hash, input_hash, workspace_hash,
                    created_at_ms
             FROM capability_recipe_run_links
             WHERE recipe_id = ?1 AND recipe_version = ?2 AND idempotency_key = ?3",
            params![link.recipe_id, link.recipe_version, link.idempotency_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing
            == (
                link.run_id.clone(),
                link.recipe_hash.clone(),
                link.template_id.clone(),
                link.template_version,
                link.template_graph_hash.clone(),
                link.run_graph_hash.clone(),
                link.input_hash.clone(),
                link.workspace_hash.clone(),
                link.created_at_ms,
            )
        {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "recipe_idempotency_conflict".into(),
        ));
    }

    transaction.execute(
        "INSERT INTO capability_recipe_run_links (
            run_id, recipe_id, recipe_version, recipe_hash, template_id,
            template_version, template_graph_hash, run_graph_hash, input_hash,
            workspace_hash, idempotency_key, created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            link.run_id,
            link.recipe_id,
            link.recipe_version,
            link.recipe_hash,
            link.template_id,
            link.template_version,
            link.template_graph_hash,
            link.run_graph_hash,
            link.input_hash,
            link.workspace_hash,
            link.idempotency_key,
            link.created_at_ms,
        ],
    )?;
    Ok(())
}

/// Loads the recipe link for one existing workflow run.
pub fn get_by_run(
    connection: &Connection,
    run_id: &str,
) -> rusqlite::Result<Option<RecipeRunLink>> {
    connection
        .query_row(
            "SELECT run_id, recipe_id, recipe_version, recipe_hash, template_id,
                    template_version, template_graph_hash, run_graph_hash,
                    input_hash, workspace_hash, idempotency_key, created_at_ms
             FROM capability_recipe_run_links WHERE run_id = ?1",
            params![run_id],
            map_link,
        )
        .optional()
}

/// Loads an immutable recipe link by its scoped idempotency key.
pub fn get_by_idempotency_key(
    connection: &Connection,
    recipe_id: &str,
    recipe_version: u32,
    idempotency_key: &str,
) -> rusqlite::Result<Option<RecipeRunLink>> {
    connection
        .query_row(
            "SELECT run_id, recipe_id, recipe_version, recipe_hash, template_id,
                    template_version, template_graph_hash, run_graph_hash,
                    input_hash, workspace_hash, idempotency_key, created_at_ms
             FROM capability_recipe_run_links
             WHERE recipe_id = ?1 AND recipe_version = ?2 AND idempotency_key = ?3",
            params![recipe_id, recipe_version, idempotency_key],
            map_link,
        )
        .optional()
}

fn map_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecipeRunLink> {
    Ok(RecipeRunLink {
        run_id: row.get(0)?,
        recipe_id: row.get(1)?,
        recipe_version: row.get(2)?,
        recipe_hash: row.get(3)?,
        template_id: row.get(4)?,
        template_version: row.get(5)?,
        template_graph_hash: row.get(6)?,
        run_graph_hash: row.get(7)?,
        input_hash: row.get(8)?,
        workspace_hash: row.get(9)?,
        idempotency_key: row.get(10)?,
        created_at_ms: row.get(11)?,
    })
}

fn validate(link: &RecipeRunLink) -> rusqlite::Result<()> {
    for (field, value, maximum) in [
        ("run_id", link.run_id.as_str(), 128),
        ("recipe_id", link.recipe_id.as_str(), 128),
        ("recipe_hash", link.recipe_hash.as_str(), 80),
        ("template_id", link.template_id.as_str(), 128),
        ("template_graph_hash", link.template_graph_hash.as_str(), 64),
        ("run_graph_hash", link.run_graph_hash.as_str(), 64),
        ("input_hash", link.input_hash.as_str(), 64),
        ("workspace_hash", link.workspace_hash.as_str(), 64),
        ("idempotency_key", link.idempotency_key.as_str(), 256),
    ] {
        if value.trim().is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                format!("{field}_empty"),
            ));
        }
        if value.len() > maximum {
            return Err(rusqlite::Error::InvalidParameterName(
                format!("{field}_too_long"),
            ));
        }
    }
    if link.recipe_version == 0 || link.template_version == 0 || link.created_at_ms < 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "recipe_run_link_bounds".into(),
        ));
    }
    if !is_sha256_hex(&link.template_graph_hash)
        || !is_sha256_hex(&link.run_graph_hash)
        || !is_sha256_hex(&link.input_hash)
        || !is_sha256_hex(&link.workspace_hash)
        || !link.recipe_hash.starts_with("sha256:")
        || !is_sha256_hex(link.recipe_hash.trim_start_matches("sha256:"))
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "recipe_run_link_hash".into(),
        ));
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
