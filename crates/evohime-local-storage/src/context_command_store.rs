//! Core-команды контекста: `pin/unpin item`, `summarize now`, аудит и rate
//! limit (этап 01.5).
//!
//! Каждая команда является mutation и получает запись аудита. Rate limit
//! считается по журналу, поэтому переживает перезапуск Core и не зависит от
//! состояния в памяти.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// Окно rate limit.
pub const RATE_LIMIT_WINDOW_MS: i64 = 60_000;
/// Максимум mutation-команд одного вида на задачу в окне.
pub const RATE_LIMIT_MAX_CALLS: usize = 30;

/// Исход команды в журнале аудита.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Applied,
    RateLimited,
    Rejected,
    /// `summarize now`: запрошено, но ещё не применено к сборке.
    Pending,
}

impl CommandOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::RateLimited => "rate_limited",
            Self::Rejected => "rejected",
            Self::Pending => "pending",
        }
    }
}

/// Запись журнала команд.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAuditRecord {
    pub id: i64,
    pub task_id: String,
    pub command: String,
    pub subject: Option<String>,
    pub outcome: String,
    pub created_at: i64,
}

/// Ошибка команды.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("rate limit exceeded for {command}: more than {max} calls per minute")]
    RateLimited { command: String, max: usize },
}

/// Хранилище команд контекста.
pub struct ContextCommandStore<'a> {
    connection: &'a Connection,
}

impl<'a> ContextCommandStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Проверка rate limit по журналу. Возвращает ошибку и пишет запись
    /// `rate_limited`, чтобы отказ тоже был виден в аудите.
    pub fn check_rate_limit(
        &self,
        task_id: &str,
        command: &str,
        now: i64,
    ) -> Result<(), StorageError> {
        let since = now.saturating_sub(RATE_LIMIT_WINDOW_MS);
        let calls: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM context_command_audit
             WHERE task_id = ?1 AND command = ?2 AND created_at >= ?3 AND outcome = 'applied'",
            rusqlite::params![task_id, command, since],
            |row| row.get(0),
        )?;
        if calls as usize >= RATE_LIMIT_MAX_CALLS {
            self.audit(task_id, command, None, CommandOutcome::RateLimited, now)?;
            return Err(StorageError::Context(
                CommandError::RateLimited {
                    command: command.to_string(),
                    max: RATE_LIMIT_MAX_CALLS,
                }
                .to_string(),
            ));
        }
        Ok(())
    }

    /// Запись аудита. Содержит только идентификаторы и исход, без содержимого.
    pub fn audit(
        &self,
        task_id: &str,
        command: &str,
        subject: Option<&str>,
        outcome: CommandOutcome,
        now: i64,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO context_command_audit (task_id, command, subject, outcome, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![task_id, command, subject, outcome.as_str(), now],
        )?;
        Ok(())
    }

    /// Журнал команд задачи, новые первыми.
    pub fn audit_log(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<CommandAuditRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, command, subject, outcome, created_at
             FROM context_command_audit WHERE task_id = ?1
             ORDER BY created_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(
            rusqlite::params![task_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| {
                Ok(CommandAuditRecord {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    command: row.get(2)?,
                    subject: row.get(3)?,
                    outcome: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// `pin/unpin item`: выставляет флаг `pinned` из 01.1. Pin повышает
    /// приоритет, но не гарантирует включение в контекст.
    pub fn set_pin(
        &self,
        task_id: &str,
        item_id: &str,
        pinned: bool,
        now: i64,
    ) -> Result<(), StorageError> {
        self.check_rate_limit(task_id, "pin_context_item", now)?;
        if pinned {
            self.connection.execute(
                "INSERT OR REPLACE INTO context_pins (task_id, item_id, pinned, updated_at)
                 VALUES (?1, ?2, 1, ?3)",
                rusqlite::params![task_id, item_id, now],
            )?;
        } else {
            self.connection.execute(
                "DELETE FROM context_pins WHERE task_id = ?1 AND item_id = ?2",
                rusqlite::params![task_id, item_id],
            )?;
        }
        self.audit(
            task_id,
            "pin_context_item",
            Some(item_id),
            CommandOutcome::Applied,
            now,
        )
    }

    /// Закреплённые item задачи.
    pub fn pinned_items(&self, task_id: &str) -> Result<Vec<String>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT item_id FROM context_pins WHERE task_id = ?1 AND pinned = 1
             ORDER BY item_id ASC",
        )?;
        let rows = statement.query_map([task_id], |row| row.get::<_, String>(0))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    /// `summarize now`: запрос действует только на следующую сборку контекста
    /// задачи и не меняет долговременную память.
    pub fn request_summarize(&self, task_id: &str, now: i64) -> Result<(), StorageError> {
        self.check_rate_limit(task_id, "summarize_now", now)?;
        self.audit(task_id, "summarize_now", None, CommandOutcome::Pending, now)
    }

    /// Забирает незакрытый запрос `summarize now`, помечая его применённым.
    pub fn take_pending_summarize(&self, task_id: &str, now: i64) -> Result<bool, StorageError> {
        let pending: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM context_command_audit
             WHERE task_id = ?1 AND command = 'summarize_now' AND outcome = 'pending'",
            [task_id],
            |row| row.get(0),
        )?;
        if pending == 0 {
            return Ok(false);
        }
        self.connection.execute(
            "UPDATE context_command_audit SET outcome = 'applied', created_at = ?2
             WHERE task_id = ?1 AND command = 'summarize_now' AND outcome = 'pending'",
            rusqlite::params![task_id, now],
        )?;
        Ok(true)
    }

    /// Очистка состояния команд задачи — часть `clear task scratchpad`.
    pub fn clear_task(&self, task_id: &str, now: i64) -> Result<(), StorageError> {
        self.connection
            .execute("DELETE FROM context_pins WHERE task_id = ?1", [task_id])?;
        self.audit(
            task_id,
            "clear_task_scratchpad",
            None,
            CommandOutcome::Applied,
            now,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalDatabase;

    fn database(name: &str) -> LocalDatabase {
        let path = std::env::temp_dir().join(format!(
            "evohime-context-command-{name}-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        LocalDatabase::open(&path).expect("database opens")
    }

    #[test]
    fn pin_and_unpin_are_persisted_and_audited() {
        let database = database("pin");
        let store = ContextCommandStore::new(database.connection());
        store
            .set_pin("task", "msg-0001-user", true, 1_000)
            .expect("pin");
        assert_eq!(
            store.pinned_items("task").expect("read"),
            vec!["msg-0001-user".to_string()]
        );
        store
            .set_pin("task", "msg-0001-user", false, 2_000)
            .expect("unpin");
        assert!(store.pinned_items("task").expect("read").is_empty());
        let audit = store.audit_log("task", 10).expect("audit");
        assert_eq!(audit.len(), 2);
        assert!(audit.iter().all(|record| record.outcome == "applied"));
    }

    #[test]
    fn every_mutation_command_gets_a_ledger_entry_in_the_audit_log() {
        let database = database("audit");
        let store = ContextCommandStore::new(database.connection());
        store
            .request_summarize("task", 1_000)
            .expect("summarize now");
        store.clear_task("task", 1_100).expect("clear");
        store.set_pin("task", "item", true, 1_200).expect("pin");
        let commands: Vec<String> = store
            .audit_log("task", 10)
            .expect("audit")
            .into_iter()
            .map(|record| record.command)
            .collect();
        assert!(commands.contains(&"summarize_now".to_string()));
        assert!(commands.contains(&"clear_task_scratchpad".to_string()));
        assert!(commands.contains(&"pin_context_item".to_string()));
    }

    #[test]
    fn the_rate_limit_rejects_excess_calls_and_records_the_refusal() {
        let database = database("rate-limit");
        let store = ContextCommandStore::new(database.connection());
        for index in 0..RATE_LIMIT_MAX_CALLS {
            store
                .set_pin("task", &format!("item-{index}"), true, 1_000)
                .expect("pin within the limit");
        }
        let error = store
            .set_pin("task", "item-over", true, 1_000)
            .expect_err("rate limit trips");
        assert!(error.to_string().contains("rate limit"));
        let refusals = store
            .audit_log("task", 100)
            .expect("audit")
            .into_iter()
            .filter(|record| record.outcome == "rate_limited")
            .count();
        assert_eq!(refusals, 1);
    }

    #[test]
    fn the_rate_limit_window_slides() {
        let database = database("rate-window");
        let store = ContextCommandStore::new(database.connection());
        for index in 0..RATE_LIMIT_MAX_CALLS {
            store
                .set_pin("task", &format!("item-{index}"), true, 1_000)
                .expect("pin within the limit");
        }
        // За пределами окна счётчик снова пуст.
        store
            .set_pin("task", "item-later", true, 1_000 + RATE_LIMIT_WINDOW_MS + 1)
            .expect("pin after the window");
    }

    #[test]
    fn a_summarize_request_is_consumed_exactly_once() {
        let database = database("summarize");
        let store = ContextCommandStore::new(database.connection());
        assert!(!store.take_pending_summarize("task", 900).expect("read"));
        store.request_summarize("task", 1_000).expect("request");
        assert!(store.take_pending_summarize("task", 1_100).expect("take"));
        assert!(!store.take_pending_summarize("task", 1_200).expect("take"));
    }

    #[test]
    fn tasks_do_not_share_pins_or_rate_limits() {
        let database = database("isolation");
        let store = ContextCommandStore::new(database.connection());
        store.set_pin("task-a", "item", true, 1_000).expect("pin");
        assert!(store.pinned_items("task-b").expect("read").is_empty());
        // Первый pin уже израсходовал одну единицу лимита задачи task-a.
        for index in 1..RATE_LIMIT_MAX_CALLS {
            store
                .set_pin("task-a", &format!("item-{index}"), true, 1_000)
                .expect("pin");
        }
        store
            .set_pin("task-b", "item", true, 1_000)
            .expect("другая задача не ограничена чужим лимитом");
    }
}
