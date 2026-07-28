# Installer PostgreSQL Bootstrap Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Исправить и проверить инициализацию встроенного PostgreSQL в установщике без запуска от администратора и без mojibake в тексте ошибок.

**Architecture:** Сохранить существующий раздел ответственности: `crates/installer/src/icacls.rs` задаёт приватные ACL для `data`, а `crates/launcher/src/postgres.rs` запускает PostgreSQL-утилиты. Текущему пользователю выдаётся наследуемое разрешение `(OI)(CI)F`; каждая дочерняя PostgreSQL-утилита получает `LC_ALL=C`.

**Tech Stack:** Rust 2021, Tokio, `icacls.exe`, встроенный PostgreSQL 16, Windows-only integration tests, Cargo.

## Global Constraints

- Работать в текущей ветке `main`; новые ветки и worktree не создавать.
- Не требовать elevation и не ослаблять приватность каталога `data`.
- Не менять порт, пользователя, пароль или схему аутентификации PostgreSQL.
- Не удалять автоматически существующий частично созданный каталог `data`.
- Не добавлять зависимости без необходимости.
- После каждой законченной части делать отдельный task-only commit.
- Удаление workspace `target/` является необязательным cleanup и не входит в критерии исправления.

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
- Produces: приватная функция `build_pg_command(exe: &Path, args: &[&str]) -> std::process::Command`, которая задаёт `LC_ALL=C`; `run_pg_tool` передаёт её в `tokio::process::Command::from_std`.

- [ ] **Step 1: Написать падающий тест конфигурации окружения**

Добавить unit-тест для выделенного builder-а команды. Тест проверяет фактическое
значение environment override через стандартный `Command` и отдельно убеждается,
что окружение самого тестового процесса не изменилось:

```rust
#[test]
fn pg_tool_command_sets_c_locale_without_changing_parent_environment() {
    let parent_locale = std::env::var_os("LC_ALL");
    let command = build_pg_command(Path::new(r"C:\EvoHime\pg16\bin\initdb.exe"), &[]);

    let locale = command
        .get_envs()
        .find(|(key, _)| key.to_string_lossy() == "LC_ALL")
        .and_then(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()));

    assert_eq!(locale.as_deref(), Some("C"));
    assert_eq!(std::env::var_os("LC_ALL"), parent_locale);
}
```

- [ ] **Step 2: Запустить тест и убедиться, что он падает до реализации**

Run: `cargo test -p evohime-launcher pg_tool_command_sets_c_locale_without_changing_parent_environment -- --exact --nocapture`

Expected: RED state because `build_pg_command` ещё не определена; после добавления
минимального builder-а тест должен перейти к обычной проверке assertion.

- [ ] **Step 3: Внести минимальную реализацию**

Добавить builder и использовать его в `run_pg_tool`:

```rust
fn build_pg_command(exe: &Path, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    command.env("LC_ALL", "C").args(args);
    command
}

let output = Command::from_std(build_pg_command(&exe, args))
    .output()
    .await?;
```

Локаль устанавливается только на дочернюю PostgreSQL-команду. Не менять окружение процесса установщика, лаунчера или самого PostgreSQL после запуска.

- [ ] **Step 4: Запустить locale-тест повторно**

Run: `cargo test -p evohime-launcher pg_tool_command_sets_c_locale_without_changing_parent_environment -- --exact --nocapture`

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

Создать явно именованный каталог под `%TEMP%`, применить новую конфигурацию ACL
с использованием существующего API `restrict_to_current_user`, а не ручного
вызова `icacls`, запустить встроенный `initdb.exe` с `-A trust -E UTF8`,
проверить наличие `PG_VERSION` и код выхода `0`. После проверки удалить только
этот созданный временный каталог, предварительно проверив его абсолютный путь и ACL.

- [ ] **Step 3: Проверить читаемость ошибки PostgreSQL**

Преднамеренно запустить встроенный `initdb.exe` с невалидным путём или непустым
тестовым каталогом через общий runner и убедиться, что возвращённый
`PgError::CommandFailed.stderr` не содержит Unicode Replacement Character
(`�`) и характерных последовательностей mojibake UTF-8→CP1251.

- [ ] **Step 4: Выполнить финальные проверки**

Run: `python C:\Users\USSR\.codex\skills\repairing-text-encoding\scripts\scan_mojibake.py .`

Run: `git diff --check`

Run: `git status --short --branch`

Expected: scanner не находит повреждённой кодировки в изменённых файлах, diff clean, рабочая копия содержит только относящиеся к задаче изменения.

- [ ] **Step 5: Необязательный cleanup Rust-артефактов**

Если нужно освободить место и активных процессов, использующих артефакты, нет,
можно удалить только workspace `target/`. Этот cleanup не является критерием
успеха исправления.

После Tasks 1–2 отдельные task-only коммиты уже содержат код. Task 3 не создаёт
пустой коммит: финальный результат передаётся после проверки `git status` и
подтверждения, что в рабочей копии нет посторонних изменений.
