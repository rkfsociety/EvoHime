# Installer PostgreSQL Bootstrap Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Исправить и проверить инициализацию встроенного PostgreSQL в установщике без запуска от администратора и без mojibake в тексте ошибок.

**Architecture:** Сохранить существующий раздел ответственности: `crates/installer/src/icacls.rs` задаёт приватные ACL для `data`, а `crates/launcher/src/postgres.rs` запускает PostgreSQL-утилиты. ACL получает наследование на файлы и каталоги через `(OI)(CI)F`; каждая дочерняя PostgreSQL-утилита получает `LC_ALL=C`.

**Tech Stack:** Rust 2021, Tokio, `icacls.exe`, bundled PostgreSQL 16, Windows-only integration tests, Cargo.

## Global Constraints

- Работать в текущей ветке `main`; новые ветки и worktree не создавать.
- Не требовать elevation и не ослаблять приватность каталога `data`.
- Не менять порт, пользователя, пароль или схему аутентификации PostgreSQL.
- Не удалять автоматически существующий частично созданный каталог `data`.
- Не добавлять зависимости без необходимости.
- После каждой законченной части делать отдельный task-only commit.
- После проверки удалить workspace `target/`, если он больше не нужен.

---

### Task 1: Исправить наследование ACL каталога данных

**Files:**
- Modify: `crates/installer/src/icacls.rs:20-31`
- Test: `crates/installer/src/icacls.rs` в существующем `#[cfg(all(test, windows))]` модуле

**Interfaces:**
- Consumes: существующая `restrict_to_current_user(dir: &Path) -> Result<(), IcaclsError>`.
- Produces: тот же публичный интерфейс; grant для текущего пользователя с `(OI)(CI)F`.

- [ ] **Step 1: Написать падающий тест доступа во вложенный каталог**

Добавить Windows-only Tokio-тест рядом с существующим тестом реального `icacls`:

```rust
#[tokio::test]
async fn grants_current_user_access_to_nested_directories() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    std::fs::create_dir(&data).unwrap();

    restrict_to_current_user(&data).await.unwrap();

    let nested = data.join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("probe.txt"), b"ok").unwrap();
}
```

- [ ] **Step 2: Запустить тест и убедиться, что он падает по исходной причине**

Run: `cargo test -p evohime-installer grants_current_user_access_to_nested_directories -- --exact --nocapture`

Expected: FAIL на `std::fs::write` с `Permission denied`, потому что текущий код выдаёт только `USERNAME:F` без наследования на вложенные каталоги.

- [ ] **Step 3: Внести минимальную реализацию**

В `restrict_to_current_user` заменить построение аргумента:

```rust
let grant_arg = format!("{username}:(OI)(CI)F");
```

Обновить doc comment, явно указав наследование на объекты и контейнеры. Не менять порядок вызовов `/inheritance:r` и `/grant:r` и не трогать другие ACL.

- [ ] **Step 4: Запустить ACL-тест повторно**

Run: `cargo test -p evohime-installer grants_current_user_access_to_nested_directories -- --exact --nocapture`

Expected: PASS; вложенный каталог создаётся, файл внутри него записывается.

- [ ] **Step 5: Запустить существующий тест формы команды `icacls`**

Run: `cargo test -p evohime-installer restricts_real_temp_directory_without_error -- --exact --nocapture`

Expected: PASS; команда `icacls` завершается с кодом `0`.

- [ ] **Step 6: Зафиксировать изолированную часть**

```powershell
git add crates/installer/src/icacls.rs
git commit -m "fix(installer): inherit PostgreSQL data ACLs"
```

### Task 2: Зафиксировать переносимую локаль PostgreSQL-утилит

**Files:**
- Modify: `crates/launcher/src/postgres.rs:186-201`
- Test: `crates/launcher/src/postgres.rs` в существующем `#[cfg(test)]` модуле

**Interfaces:**
- Consumes: существующая `run_pg_tool(pg_bin_dir: &Path, tool: &str, args: &[&str]) -> Result<(), PgError>`.
- Produces: те же функции `initdb`, `start` и `stop`; каждый дочерний PostgreSQL-процесс получает `LC_ALL=C`.

- [ ] **Step 1: Написать падающий тест конфигурации окружения**

Добавить Windows-only Tokio-тест, который вызывает существующий `run_pg_tool`
через `cmd.exe`. Тест временно задаёт родительскую `LC_ALL=ru_RU`, просит
`cmd.exe` напечатать значение переменной в `stderr` и завершиться с кодом `1`:

```rust
#[tokio::test]
async fn postgres_tool_forces_c_locale() {
    let previous = std::env::var_os("LC_ALL");
    std::env::set_var("LC_ALL", "ru_RU");

    let comspec = std::env::var_os("ComSpec").unwrap();
    let bin_dir = std::path::Path::new(&comspec).parent().unwrap();
    let result = run_pg_tool(
        bin_dir,
        "cmd",
        &["/C", "echo %LC_ALL% 1>&2 & exit /b 1"],
    )
    .await;

    match previous {
        Some(value) => std::env::set_var("LC_ALL", value),
        None => std::env::remove_var("LC_ALL"),
    }

    let error = result.unwrap_err();
    let stderr = match error {
        PgError::CommandFailed { stderr, .. } => stderr,
        other => panic!("expected command failure, got {other:?}"),
    };
    assert!(stderr.contains('C'), "stderr was: {stderr:?}");
}
```

- [ ] **Step 2: Запустить тест и убедиться, что он падает до реализации**

Run: `cargo test -p evohime-launcher postgres_tool_forces_c_locale -- --exact --nocapture`

Expected: FAIL because `run_pg_tool` currently inherits `LC_ALL=ru_RU`, so
`stderr` does not contain the required `C` value.

- [ ] **Step 3: Внести минимальную реализацию**

Добавить `.env("LC_ALL", "C")` к дочерней команде в `run_pg_tool` перед
передачей аргументов:

```rust
let output = Command::new(&exe)
    .env("LC_ALL", "C")
    .args(args)
    .output()
    .await?;
```

Локаль устанавливается только на дочернюю PostgreSQL-команду. Не менять окружение процесса установщика, лаунчера или самого PostgreSQL после запуска.

- [ ] **Step 4: Запустить locale-тест повторно**

Run: `cargo test -p evohime-launcher postgres_tool_forces_c_locale -- --exact --nocapture`

Expected: PASS; найдено значение `LC_ALL=C`.

- [ ] **Step 5: Зафиксировать изолированную часть**

```powershell
git add crates/launcher/src/postgres.rs
git commit -m "fix(postgres): use portable locale for tool diagnostics"
```

### Task 3: Полная проверка и ручная регрессия `initdb`

**Files:**
- Verify: `crates/installer/src/icacls.rs`, `crates/launcher/src/postgres.rs`
- Verify: `docs/superpowers/specs/2026-07-28-installer-postgres-bootstrap-fix-design.md`

**Interfaces:**
- Consumes: изменения из Tasks 1–2.
- Produces: доказательство успешного bootstrap и чистое состояние рабочей копии.

- [ ] **Step 1: Запустить форматирование и целевые тесты**

Run: `cargo fmt --check`

Run: `cargo test -p evohime-installer`

Run: `cargo test -p evohime-launcher`

Expected: все команды завершаются с кодом `0`.

- [ ] **Step 2: Повторить исходный сценарий на чистом временном каталоге**

Создать явно именованный каталог под `%TEMP%`, применить исправленную ACL через `restrict_to_current_user`, запустить встроенный `initdb.exe` с `-A trust -E UTF8`, проверить наличие `PG_VERSION` и код выхода `0`. После проверки удалить только этот созданный временный каталог, предварительно проверив его абсолютный путь и ACL.

- [ ] **Step 3: Проверить читаемость ошибки PostgreSQL**

Преднамеренно запустить встроенный `initdb.exe` с невалидным путём или непустым тестовым каталогом через общий runner и убедиться, что возвращённый `PgError::CommandFailed.stderr` содержит англоязычный текст без `�`, `Рџ`, `СЃ` и других mojibake-фрагментов.

- [ ] **Step 4: Выполнить финальные проверки**

Run: `python C:\Users\USSR\.codex\skills\repairing-text-encoding\scripts\scan_mojibake.py .`

Run: `git diff --check`

Run: `git status --short --branch`

Expected: scanner не находит повреждённой кодировки в изменённых файлах, diff clean, рабочая копия содержит только относящиеся к задаче изменения.

- [ ] **Step 5: Удалить ненужные Rust-артефакты**

Если после проверки не требуется продолжать работу с собранными артефактами, удалить только workspace `target/` и проверить, что активных процессов, использующих его, нет.

- [ ] **Step 6: Зафиксировать проверку**

```powershell
git add docs/superpowers/plans/2026-07-28-installer-postgres-bootstrap-fix.md
git commit -m "docs(installer): add PostgreSQL bootstrap fix plan"
```
