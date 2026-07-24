# Browser session Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Первая волна `7.100`: CDP-клиент с persistent-сессией на задачу и инструменты `browser.session.navigate|read|click|close`.

**Architecture:** `crates/tool-runtime/src/cdp.rs` — минимальный CDP-клиент (DevTools HTTP API для create/close target, websocket для команд) и реестр сессий `task_id → CdpSession` (once_cell, кап 4, idle 10 минут). `tools/browser_session.rs` — четыре инструмента поверх клиента. Конфиг `EVOHIME_BROWSER_CDP_URL`; без него — понятная ошибка.

**Tech Stack:** Rust, tokio-tungstenite, reqwest (уже в deps), wiremock + локальный ws-сервер в тестах.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Никакого произвольного JS от модели; только фиксированные выражения инструментов.
- SSRF-валидация целевых URL навигации; кап/таймауты обязательны.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: CDP client + session registry

**Files:**
- Create: `crates/tool-runtime/src/cdp.rs`
- Modify: `crates/tool-runtime/src/lib.rs`, `crates/tool-runtime/Cargo.toml` (tokio-tungstenite)

**Interfaces:**
- `cdp_base_url() -> Option<String>` из `EVOHIME_BROWSER_CDP_URL`
- `CdpSession::open(base, timeout)` — PUT `/json/new`, ws connect, `Page.enable`
- `session.navigate(url, timeout)`, `session.page_snapshot(max_chars)`, `session.click(selector, settle)`, `session.close()`
- `session_for_task(task_id)` / `take_session(task_id)` — реестр с капом и idle-вытеснением

- [ ] **Step 1: Unit tests first** — сериализация команд, разбор response/event кадров, кап и idle реестра (без сети — на фейковом транспорте или чистых функциях).
- [ ] **Step 2: Implement client** — command id counter, send/await matching id, попутные события в flags (`load_fired`); таймаут каждой команды.
- [ ] **Step 3: Mock integration test** — wiremock `/json/new` + tokio-tungstenite server: navigate/evaluate round-trip.
- [ ] **Step 4: Checks** — `cargo test -p evohime-tool-runtime cdp`, Clippy.
- [ ] **Step 5: Commit** — `feat(tools): add CDP client with per-task sessions`.

### Task 2: browser.session tools

**Files:**
- Create: `crates/tool-runtime/src/tools/browser_session.rs`
- Modify: `crates/tool-runtime/src/tools/mod.rs`, `crates/tool-runtime/src/registry.rs`

- [ ] **Step 1: Tools** — navigate (SSRF-check → session reuse → navigate → snapshot), read, click (selector not found → InvalidInput), close; структурированный результат как у других tools.
- [ ] **Step 2: Registry** — четыре `ToolDefinition` (BrowserAccess, timeout 30s) + dispatch.
- [ ] **Step 3: Tests** — mock CDP: navigate → click → read в одной сессии; ошибка без `EVOHIME_BROWSER_CDP_URL`.
- [ ] **Step 4: Checks** — `cargo test -p evohime-tool-runtime`, Clippy workspace.
- [ ] **Step 5: Commit** — `feat(tools): add browser session tools`.

### Task 3: Документация

**Files:**
- Modify: `AGENTS.md` (tools list), `docs/roadmap.md` (матрица инструментов + `7.100` notes), `docs/current-state.md`, `.env.example`

- [ ] **Step 1: Docs** — `EVOHIME_BROWSER_CDP_URL` в env; `7.100` → 🟡 wave 1; матрица инструментов.
- [ ] **Step 2: Commit** — `docs: mark browser session wave complete`.
