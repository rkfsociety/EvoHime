# Plugin lock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Вторая волна `7.102`: `.evohime/plugins.lock.json` с content-hash каждого установленного плагина и `GET /api/plugins/integrity` со статусами ok/modified/unlocked/missing + chip в PluginsPanel.

**Architecture:** `crates/server/src/plugin_lock.rs` — детерминированный `content_hash(dir)` (sorted walk, skip `.git`/симлинки, SHA-256 путь+размер+байты), `PluginLockEntry`, atomic load/save, record/remove. `plugins.rs`: запись lock после install, удаление на uninstall, endpoint integrity. Frontend: chip в installed-списке.

**Tech Stack:** Rust (sha2 уже в deps), React/TS.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Проверка read-only; повреждённый lock → `unlocked` + warning, не 500.
- Атомарная запись lock (tmp + rename).
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: plugin_lock module

**Files:**
- Create: `crates/server/src/plugin_lock.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Unit tests first** — hash детерминирован и не зависит от порядка создания; правка байта/переименование меняют hash; `.git` игнорируется; load/save round-trip; повреждённый JSON → пустая map + flag.
- [ ] **Step 2: Implement** — `content_hash`, `PluginLockEntry`, `load_lock -> (map, corrupted)`, `save_lock`, `record_install`, `remove_entry`.
- [ ] **Step 3: Checks** — `cargo test -p evohime-server plugin_lock`, Clippy.
- [ ] **Step 4: Commit** — `feat(plugins): add content-hash lock file`.

### Task 2: Integrity endpoint + install wiring

**Files:**
- Modify: `crates/server/src/plugins.rs`, `crates/server/src/routes.rs`
- Regenerate: `docs/openapi.json`, `frontend/web/src/api/generated.ts`

- [ ] **Step 1: Wiring** — install/update: после сканирования записать lock entry (hash, trust.level); uninstall: удалить entry.
- [ ] **Step 2: Endpoint** — `GET /api/plugins/integrity`: статусы ok/modified/unlocked/missing, `lock_corrupted` warning.
- [ ] **Step 3: Tests** — integration на temp workspace: записать entry → ok; правка → modified; удаление каталога → missing; посторонний каталог → unlocked.
- [ ] **Step 4: Checks** — `cargo test -p evohime-server`, Clippy workspace, OpenAPI regen.
- [ ] **Step 5: Commit** — `feat(plugins): report install integrity`.

### Task 3: UI + документация

**Files:**
- Modify: `frontend/web/src/api/plugins.ts`, `frontend/web/src/panels/PluginsPanel.tsx`, `frontend/web/src/styles/plugins-sites.css`
- Modify: `AGENTS.md`, `docs/roadmap.md`, `docs/current-state.md`

- [ ] **Step 1: UI** — загрузка integrity, chip ok/modified/unlocked/missing у установленных плагинов.
- [ ] **Step 2: Checks** — build, workspace test.
- [ ] **Step 3: Docs** — `7.102` ✅ (подписи/репутация зафиксированы как вне пункта — нет внешней инфраструктуры).
- [ ] **Step 4: Commit** — `feat(ui): show plugin integrity` и `docs: mark plugin lock complete`.
