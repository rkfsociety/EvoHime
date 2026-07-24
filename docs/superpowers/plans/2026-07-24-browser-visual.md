# Browser visual Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Вторая волна `7.100`: `browser.session.screenshot` (PNG в workspace sandbox) и `browser.session.type` (ввод в формы через фиксированный JS).

**Architecture:** `CdpSession` получает `capture_screenshot()` (`Page.captureScreenshot` → base64) и `type_text(selector, text)` (фиксированное выражение, JSON-литералы). Инструменты в `browser_session.rs`: screenshot декодирует base64 (лимит 16 MiB) и пишет через sandbox `resolve_for_write`; type ограничивает текст 16 KiB и не возвращает его в structured/логи.

**Tech Stack:** Rust, base64 crate, существующий CDP-клиент/реестр, mock-CDP тесты.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Никакого произвольного JS; текст/селектор только как JSON-литералы.
- Запись только внутрь sandbox; лимиты размера обязательны; text не логируется.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: CDP methods + tools

**Files:**
- Modify: `crates/tool-runtime/src/cdp.rs`, `crates/tool-runtime/src/tools/browser_session.rs`, `crates/tool-runtime/src/registry.rs`, `crates/tool-runtime/Cargo.toml` (base64)

- [ ] **Step 1: CDP** — `capture_screenshot()` → base64 String; `type_text(selector, text)` → bool found (focus + value/textContent + dispatch input/change).
- [ ] **Step 2: Tools** — screenshot (default path `.evohime/screenshots/<task>-<ts>.png`, форс `.png`, decode limit 16 MiB, sandbox write) и type (лимит 16 KiB, settle, snapshot; structured содержит только `text_length`).
- [ ] **Step 3: Registry** — определения + dispatch; счётчик в тесте 22 → 24.
- [ ] **Step 4: Mock tests** — screenshot пишет файл; type меняет mock-состояние и виден в read; отсутствующий селектор → InvalidInput; oversize text → InvalidInput.
- [ ] **Step 5: Checks** — `cargo test -p evohime-tool-runtime`, Clippy.
- [ ] **Step 6: Commit** — `feat(tools): add browser screenshot and typing`.

### Task 2: Agent loop + документация

**Files:**
- Modify: `crates/agent-runtime/src/agent_loop/parse.rs`, `crates/agent-runtime/src/native_tools.rs`
- Modify: `AGENTS.md`, `docs/roadmap.md`, `docs/current-state.md`

- [ ] **Step 1: Agent loop** — `REGISTERED_TOOLS` + JSON-схемы для screenshot/type.
- [ ] **Step 2: Checks** — `cargo test --workspace`, Clippy, frontend build.
- [ ] **Step 3: Docs** — матрица инструментов, `7.100` wave 2 ✅ (остаток — vision-вход/поэлементные скриншоты), tools list в AGENTS.md.
- [ ] **Step 4: Commit** — `feat(agent): expose browser visual tools` и `docs: mark browser visual wave complete`.
