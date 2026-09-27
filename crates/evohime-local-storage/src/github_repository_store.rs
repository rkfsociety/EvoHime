//! Durable identifiers for public GitHub repositories explicitly selected by the user.

use rusqlite::{params, Connection};

const MAX_SAVED_REPOSITORIES: i64 = 50;

/// A public GitHub repository saved in this local database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedGitHubRepository {
    /// Account or organization that owns the repository.
    pub owner: String,
    /// Repository name without a `.git` suffix.
    pub repo: String,
    /// Time when the identifier was saved, in Unix milliseconds.
    pub created_at_ms: i64,
}

/// Installs the saved-repository schema if it is not already present.
pub(crate) fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS github_saved_repositories (
           owner TEXT NOT NULL COLLATE NOCASE CHECK(length(CAST(owner AS BLOB)) BETWEEN 1 AND 39),
           repo TEXT NOT NULL COLLATE NOCASE CHECK(length(CAST(repo AS BLOB)) BETWEEN 1 AND 100),
           created_at_ms INTEGER NOT NULL,
           PRIMARY KEY(owner, repo)
         );",
    )
}

/// Lists repositories in a stable case-insensitive order.
pub fn list(connection: &Connection) -> Result<Vec<SavedGitHubRepository>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT owner, repo, created_at_ms FROM github_saved_repositories
         ORDER BY owner COLLATE NOCASE, repo COLLATE NOCASE LIMIT ?1",
    )?;
    let rows = statement.query_map([MAX_SAVED_REPOSITORIES], |row| {
        Ok(SavedGitHubRepository {
            owner: row.get(0)?,
            repo: row.get(1)?,
            created_at_ms: row.get(2)?,
        })
    })?;
    rows.collect()
}

/// Saves one repository identifier, returning `false` when it already exists.
///
/// # Errors
///
/// Returns a database error if persistence fails or the local list is full.
pub fn save(
    connection: &Connection,
    owner: &str,
    repo: &str,
    created_at_ms: i64,
) -> Result<bool, rusqlite::Error> {
    let transaction = connection.unchecked_transaction()?;
    let count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM github_saved_repositories",
        [],
        |row| row.get(0),
    )?;
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM github_saved_repositories WHERE owner=?1 AND repo=?2)",
        params![owner, repo],
        |row| row.get(0),
    )?;
    if !exists && count >= MAX_SAVED_REPOSITORIES {
        return Err(rusqlite::Error::InvalidParameterName(
            "github_repository_limit".into(),
        ));
    }
    let inserted = if exists {
        false
    } else {
        transaction.execute(
            "INSERT INTO github_saved_repositories(owner,repo,created_at_ms) VALUES (?1,?2,?3)",
            params![owner, repo, created_at_ms],
        )? == 1
    };
    transaction.commit()?;
    Ok(inserted)
}

/// Removes one saved repository identifier, returning whether it existed.
pub fn remove(connection: &Connection, owner: &str, repo: &str) -> Result<bool, rusqlite::Error> {
    Ok(connection.execute(
        "DELETE FROM github_saved_repositories WHERE owner=?1 AND repo=?2",
        params![owner, repo],
    )? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().expect("database");
        install_schema(&connection).expect("schema");
        connection
    }

    #[test]
    fn saved_repository_crud_is_case_insensitive_and_bounded() {
        let connection = database();
        assert!(save(&connection, "Owner", "Repo", 7).expect("save"));
        assert!(!save(&connection, "owner", "repo", 8).expect("duplicate"));
        assert_eq!(list(&connection).expect("list").len(), 1);
        assert!(remove(&connection, "OWNER", "REPO").expect("remove"));
        assert!(!remove(&connection, "owner", "repo").expect("remove absent"));

        for index in 0..MAX_SAVED_REPOSITORIES {
            let repo = format!("repo-{index}");
            assert!(save(&connection, "owner", &repo, index).expect("bounded save"));
        }
        assert!(save(&connection, "owner", "repo-over-limit", 100).is_err());
        assert!(!save(&connection, "OWNER", "REPO-0", 101).expect("duplicate at limit"));
    }
}
