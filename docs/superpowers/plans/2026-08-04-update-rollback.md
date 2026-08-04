# Надёжное обновление с rollback — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сделать обновление Евы транзакционным: при ошибке, аварийном завершении или незавершённой предыдущей установке автоматически восстанавливать рабочую версию.

**Architecture:** Отдельный Rust-компонент `evohime-transaction.exe` создаёт backup фиксированного набора файлов, атомарно записывает журнал транзакции, запускает Inno Setup и выполняет commit либо rollback. Supervisor перед запуском Core вызывает recovery того же журнала, поэтому восстановление работает и после внезапного завершения transaction worker.

**Tech Stack:** Rust workspace, serde/serde_json, Windows process API через `std::process::Command`, WinUI 3/C#, Inno Setup, PowerShell CI smoke tests.

## Global Constraints

- Пользовательский релиз публикует только один `EvoHime-Setup.exe`.
- Пользовательский ярлык остаётся единственным ярлыком `EvoHime`.
- Backup содержит только `EvoHime.exe`, `evohime-core.exe`, `evohime-supervisor.exe`, `evohime.manifest.json`.
- Журнал и backup находятся в `%LOCALAPPDATA%\EvoHime\update-state`, вне каталога установки.
- Все изменения выполняются в текущей ветке `main`; новые ветки и worktree не создаются.
- После каждой законченной задачи запускаются её тесты и создаётся task-коммит.

---

### Task 1: Транзакционное хранилище updater

**Files:**
- Create: `crates/evohime-updater/Cargo.toml`
- Create: `crates/evohime-updater/src/lib.rs`
- Modify: `Cargo.toml`
- Test: `crates/evohime-updater/src/lib.rs` (unit tests)

**Interfaces:**
- Produces `UpdateTransaction::prepare`, `commit`, `rollback` and `recover` для updater и supervisor.
- `UpdateTransaction::prepare(install_dir: &Path, state_dir: &Path) -> io::Result<Self>` создаёт backup и state-файл.
- `UpdateTransaction::commit(self) -> io::Result<()>` удаляет state и backup.
- `UpdateTransaction::rollback(&self) -> io::Result<()>` восстанавливает компоненты и удаляет state.
- `recover(state_dir: &Path) -> io::Result<RecoveryResult>` восстанавливает любую незавершённую транзакцию.

- [ ] **Step 1: Write failing tests** для prepare/commit, rollback после подмены файла, отсутствующего компонента и recovery после оставленного state-файла.
- [ ] **Step 2: Run tests and verify they fail**: `cargo test --locked -p evohime-updater`.
- [ ] **Step 3: Implement fixed component list, JSON state and atomic state write** через временный файл и `rename`.
- [ ] **Step 4: Run tests and verify they pass**: `cargo test --locked -p evohime-updater`.
- [ ] **Step 5: Commit**: `git commit -m "feat: add update transaction storage"`.

### Task 2: Исполняемый updater

**Files:**
- Modify: `crates/evohime-updater/Cargo.toml`
- Create: `crates/evohime-updater/src/main.rs`
- Modify: `crates/evohime-updater/src/lib.rs`
- Test: `crates/evohime-updater/src/lib.rs` (process command tests where platform permits)

**Interfaces:**
- CLI: `evohime-updater.exe --installer <path> --install-dir <path> --state-dir <path>`.
- Exit code `0` означает commit, ненулевой код — rollback и ненулевой результат.
- Перед запуском installer вызывается `UpdateTransaction::prepare`; после успеха проверяются все fixed components.

- [ ] **Step 1: Write failing tests** для валидации абсолютных путей, проверки обязательных компонентов и ненулевого результата installer.
- [ ] **Step 2: Run targeted tests and verify the failure.**
- [ ] **Step 3: Implement CLI argument parsing, child process wait, verify, commit and rollback.**
- [ ] **Step 4: Run `cargo test --locked -p evohime-updater`.**
- [ ] **Step 5: Commit**: `git commit -m "feat: add transactional update runner"`.

### Task 3: Recovery в supervisor

**Files:**
- Modify: `crates/evohime-supervisor/Cargo.toml`
- Modify: `crates/evohime-supervisor/src/windows_supervisor.rs`
- Test: `crates/evohime-supervisor/src/windows_supervisor.rs` or `crates/evohime-supervisor/tests/recovery.rs`

**Interfaces:**
- Supervisor получает `EVOHIME_UPDATE_STATE_DIR` или использует `%LOCALAPPDATA%\EvoHime\update-state`.
- Перед mutex/core loop вызывает `evohime_updater::recover`; при ошибке пишет structured log и не запускает Core поверх неизвестного состояния.

- [ ] **Step 1: Write failing recovery integration test** с незавершённым state и изменённым installed component.
- [ ] **Step 2: Run supervisor/updater tests and verify failure.**
- [ ] **Step 3: Add updater dependency and recovery call before Core.**
- [ ] **Step 4: Run `cargo test --locked -p evohime-supervisor -p evohime-updater`.**
- [ ] **Step 5: Commit**: `git commit -m "feat: recover interrupted updates before core"`.

### Task 4: Подключение WinUI и единый пакет

**Files:**
- Modify: `desktop/EvoHime.Desktop/Services/UpdateService.cs`
- Modify: `desktop/EvoHime.Desktop/MainWindow.xaml.cs`
- Modify: `scripts/build-windows-native.ps1`
- Modify: `installer/EvoHime.iss`
- Modify: `scripts/native-package.tests.ps1`
- Modify: `.github/workflows/rust.yml`
- Modify: `scripts/native-workflow.tests.ps1`
- Test: `desktop/EvoHime.Tests/IpcCompatibilityTests.cs`

**Interfaces:**
- UI запускает `evohime-updater.exe`, а не устанавливает файл напрямую.
- Пакет содержит updater как скрытый компонент; артефакт CI остаётся одним `EvoHime-Setup.exe`.
- CI после native checks выполняет install, upgrade, forced-failure rollback и recovery smoke.

- [ ] **Step 1: Write failing C# and PowerShell tests** на запуск updater и наличие updater в package.
- [ ] **Step 2: Run tests and verify failure.**
- [ ] **Step 3: Add updater to native build/package and route UpdateService through it.**
- [ ] **Step 4: Add CI rollback smoke using a deterministic failing installer stub and a leftover state file.**
- [ ] **Step 5: Run WinUI tests and all native smoke scripts.**
- [ ] **Step 6: Commit**: `git commit -m "feat: wire rollback into native release"`.

### Task 5: Финальная проверка и уборка

**Files:**
- Modify: `docs/current-state.md`
- Modify: `docs/development-plan.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Run Rust, WinUI, package, workflow, version and installer smoke tests.**
- [ ] **Step 2: Build versioned package and verify internal version equals tag.**
- [ ] **Step 3: Run `git diff --check`, remove `target/`, `artifacts/`, `bin/`, `obj/`.**
- [ ] **Step 4: Update canonical docs with rollback status.**
- [ ] **Step 5: Commit**: `git commit -m "docs: mark update rollback release ready"`.
