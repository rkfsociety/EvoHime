use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 36 {
        let columns = t.prepare("PRAGMA table_info(continuation_runs)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|column| column == "prompt") {
            t.execute_batch("ALTER TABLE continuation_runs ADD COLUMN prompt TEXT;")?;
        }
        if !columns.iter().any(|column| column == "workspace_path") {
            t.execute_batch("ALTER TABLE continuation_runs ADD COLUMN workspace_path TEXT;")?;
        }
        t.execute_batch("PRAGMA user_version = 36;")?;
    }
    Ok(())
}
