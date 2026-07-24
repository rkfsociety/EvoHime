# Plugin trust Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Первая волна `7.102`: trust score из проверяемых сигналов для каждого элемента каталога плагинов + статический risk scan при установке с отказом для недоверенных рискованных плагинов (`force` — осознанный override).

**Architecture:** `crates/server/src/plugin_trust.rs` — чистые функции `assess_trust(source, entry-поля)` и `scan_plugin_dir(path)`. `plugins.rs`: `ResolvedCatalogEntry.catalog_source`, `CatalogPlugin.trust`, install-гейт после клона, `force` в запросе, `risk_findings` в ответе. Frontend: trust-бейдж в PluginsPanel.

**Tech Stack:** Rust (без новых deps), React/TS для бейджа.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Scan не исполняет код, не следует симлинкам, bounded по файлам/размеру.
- Официальные источники не ломаются; отказ только для findings у ниже-official без force.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: plugin_trust module

**Files:**
- Create: `crates/server/src/plugin_trust.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Unit tests first** — ярусы источников (anthropics URL → official, DEFAULT_CATALOG_SOURCES → curated, прочее → community), commit-pin vs branch vs none, уровни по score; scan: `curl | sh` найден, чистый файл — нет, бинарь помечен, лимит файлов соблюдён.
- [ ] **Step 2: Implement** — `TrustScore { score, level, reasons }`, `assess_trust`; `RiskFinding { file, pattern, excerpt }`, `scan_plugin_dir` (walkdir вручную через std, max 500 файлов / 512 KiB на файл, симлинки скипаются).
- [ ] **Step 3: Checks** — `cargo test -p evohime-server plugin_trust`, Clippy.
- [ ] **Step 4: Commit** — `feat(plugins): add trust scoring and risk scan`.

### Task 2: Каталог + install-гейт

**Files:**
- Modify: `crates/server/src/plugins.rs`

- [ ] **Step 1: catalog_source** — протащить URL источника в `ResolvedCatalogEntry` при парсинге; `CatalogPlugin.trust` из `assess_trust`.
- [ ] **Step 2: Install-гейт** — после клона `scan_plugin_dir`; findings + уровень ниже official + `!force` → удалить клон, `BadRequest` с перечнем; иначе `risk_findings` в `InstalledPlugin`.
- [ ] **Step 3: Tests** — существующие plugins-тесты обновить; новый тест гейта на temp-дереве.
- [ ] **Step 4: Checks** — `cargo test -p evohime-server plugins`, Clippy workspace.
- [ ] **Step 5: Commit** — `feat(plugins): gate risky installs by trust`.

### Task 3: Frontend + документация

**Files:**
- Modify: `frontend/web/src/panels/PluginsPanel.tsx`, `frontend/web/src/api/*` (тип CatalogPlugin)
- Modify: `AGENTS.md`, `docs/roadmap.md`, `docs/current-state.md`

- [ ] **Step 1: UI** — trust-бейдж (уровень + score, tooltip с reasons) в каталоге.
- [ ] **Step 2: Checks** — typecheck/build, workspace test.
- [ ] **Step 3: Docs** — `7.102` 🟡 wave 1 ✅ (остаток — подписи, репутация, рейтинги).
- [ ] **Step 4: Commit** — `feat(ui): show plugin trust badges` и `docs: mark plugin trust wave complete`.
