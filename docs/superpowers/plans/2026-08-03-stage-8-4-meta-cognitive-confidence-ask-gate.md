# Stage 8.4 — Meta-cognitive confidence сигнал в ask-gate

**Дата:** 2026-08-03  
**Размер:** M (~3–7 дней)  
**Зависимости:** ask-on-uncertainty gate (`6.20`), experience memory (`6.21`), reflection loop (`8.2`)  
**Статус:** Plan

## Задача

Расширить текущий `ask-on-uncertainty` gate из 6.20 более богатым сигналом уверенности агента. Вместо бинарного uncertainty-threshold, вводим multi-dimensional confidence score, который отражает:

1. **Model confidence** — как сам язык-модель оценивает собственную уверенность (через логиты, вероятности токенов или явный Thinking-выход)
2. **Experience alignment** — насколько похожа текущая задача на прошлые успешные решения (cosine similarity + confidence из memory)
3. **Tool success rate** — статистика прошлых вызовов инструментов, которые агент собирается использовать
4. **Reflection feedback** — сигналы из self-reflection loop (8.2): количество ошибок, rate of revision, confidence decay при повторах
5. **High-impact signal** — есть ли в плане шаги, помеченные как опасные (filesystem.write, git.push, shell с суперправами)

## Критерии выполнения

### 🔴 Архитектурные (ОБЯЗАТЕЛЬНЫЕ)

- [ ] **Разделить Confidence и Risk на независимые оси** (`crates/agent-runtime/src/agent_loop/ask_gate.rs`)
  - `confidence_score: f32` — [0.0, 1.0], отражает уверенность агента в решении
  - `risk_level: enum` {None, Low, Medium, High} — определяется по типам инструментов в плане (write, push, shell-dangerous)
  - `ask_policy(confidence, risk)` — функция, которая независимо решает:
    - `if risk >= High: require_approval()` regardless of confidence
    - `elif confidence >= threshold[risk]: proceed`
    - `else: ask()`
  - Risk-aware thresholds (в конфиге):
    - High risk: требует `confidence >= 0.85` (вместо 0.75)
    - Medium risk: требует `confidence >= 0.75`
    - Low/None: требует `confidence >= 0.65`

- [ ] **Нормализовать веса сигналов** (сумма всегда = 1.0)
  - `confidence = 0.35 * model_conf + 0.25 * exp_align + 0.25 * tool_rate + 0.15 * reflection_conf`
  - High-impact не входит в формулу; оценивается отдельно как `risk_level`

- [ ] **Model confidence с reliability-уровнем** (`crates/agent-runtime/src/agent_loop/model_confidence.rs` — новый)
  - Detection: спрашивать у provider capability: `supports_logprobs`, `supports_thinking`
  - Приоритет источников (в порядке убывания надёжности):
    1. Logprobs из streaming ответа (LiteRouter/OpenAI) → `reliability: high`
    2. Structured output: `{"confidence": 0.8}` из system prompt → `reliability: medium`
    3. Thinking token count / total (для Claude) → `reliability: low` (thinking≠confidence)
    4. Keyword heuristics ("maybe", "perhaps", "I'm not sure") → `reliability: very_low`
  - Fallback: `model_confidence = 0.5, reliability = low` (не оптимистичный fallback)
  - Плохой сигнал (reliability < medium) → понижает итоговую confidence на -0.1

- [ ] **Fail-closed при отсутствующих сигналах** (`ask_gate.rs`)
  - Track `missing_signals[]`: какие именно сигналы отсутствуют (no_memory_history, no_tool_stats, no_reflection)
  - Policy: `if missing_signals.len() >= 2: ask()` (консервативно)
  - Emit в событие `agent.confidence {missing_signals}` для audit

### 📊 Сигналы (детализированные формулы)

- [ ] **Tool success rate с агрегацией и сглаживанием** (`crates/storage/src/tool_metrics.rs`)
  - Schema: `tool_execution_stats(tool_name, operation_type, success: bool, created_at, ...)`
  - Для каждого planned tool: `SELECT COUNT(success=true) as s, COUNT(*) as n FROM ... WHERE tool_name=? AND created_at > now()-30d`
  - Сглаживание: `smoothed_rate = (s + 1) / (n + 2)` (beta-binomial prior, α=β=1)
  - При `n < 5`: помечать reliability=low, не давать полный вес в итоговый score
  - Разделять read-only vs destructive: для `git.push` / `filesystem.write` требуется отдельная история, не смешивать со статистикой `grep`
  - Если несколько tools в плане: использовать консервативный минимум (наихудший из всех)

- [ ] **Reflection confidence с типизацией ревизий** (расширение `8.2`)
  - Schema: `reflection_events` добавить `revision_type: enum {minor, major, repeated_failure}`
  - Формула: `reflection_confidence = 1.0 - clamp(0.0, 1.0, (major_revisions*0.3 + repeated*0.5) / max(step_count, 1))`
  - Старые ошибки со временем "забываются" (exponential decay к 1.0)

- [ ] **Experience alignment с явной формулой** (изменить `crates/memory/src/retrieve.rs`)
  - Запрашивать у retrieval: top-k=3 похожих playbooks с их (similarity: f32, confidence_at_creation: f32, recency_score: f32)
  - Если похожих < 2: alignment = 0.5 (uncertain)
  - Если есть: `experience_alignment = (Σ similarity[i] * confidence[i] * recency[i]) / (Σ similarity[i])`, клампировать к [0.0, 1.0]
  - Нужна миграция: при сохранении опыта в memory, писать `model_confidence_at_creation` и `created_at`

- [ ] **Risk level determination** (`crates/agent-runtime/src/agent_loop/risk_engine.rs` — новый)
  - На основе planned steps, анализировать:
    - Уровень 0 (None): только read-only tools (`filesystem.read`, `git.status`, `browser.open`)
    - Уровень 1 (Low): создание файлов (`filesystem.write` в temp/logs)
    - Уровень 2 (Medium): изменение кода (`filesystem.patch`, `git.commit`)
    - Уровень 3 (High): пуш в репо (`git.push`), shell без whitelist (`shell.execute` + полная команда без проверки)
  - Детализация: анализировать аргументы/пути, не только имя tool
  - Результат: наихудший risk из всех шагов

### 🔧 Инфраструктура

- [ ] **Schema & миграции** (`migrations/0035_confidence_signals.sql`)
  - `tool_execution_stats(id, tool_name, operation_type, success, error_category, task_id, created_at, index on (tool_name, created_at))`
  - `memory_items` — добавить `model_confidence_at_creation: f32 NULL`, миграция заполнить дефолтом 0.5 для старых записей
  - `reflection_events` — добавить `revision_type: text, confidence_delta: f32 NULL`
  - `confidence_audit_log(id, task_id, confidence_score, risk_level, breakdown jsonb, missing_signals text[], decision: text, timestamp)` — для audit trail

- [ ] **Конфигурация** (env + schema)
  - `EVOHIME_CONFIDENCE_GATE_ENABLED=1` (default true)
  - `EVOHIME_CONFIDENCE_THRESHOLDS` (JSON):
    ```json
    {
      "none": {"proceed": 0.65, "ask": 0.40},
      "low": {"proceed": 0.70, "ask": 0.45},
      "medium": {"proceed": 0.75, "ask": 0.50},
      "high": {"proceed": 0.85, "ask": 0.65, "require": 0.30}
    }
    ```
  - `EVOHIME_CONFIDENCE_TOOL_MIN_HISTORY=5` — при меньшем числе примеров, tool_rate помечается low-reliability
  - `EVOHIME_CONFIDENCE_MISSING_SIGNAL_THRESHOLD=2` — при ≥2 отсутствующих сигналах, ask() обязателен

- [ ] **Event schema** (`protocol/schema/evohime.protocol.schema.json`)
  - `AgentConfidence {version: "1", confidence_score: f32, risk_level: string, breakdown: {model, experience, tools, reflection}, missing_signals: [string], reliability: {model, experience, tools, reflection}, recommendation: "proceed" | "ask" | "require", timestamp}`
  - Версионирование: инкремент `version` при изменении структуры; старый код игнорирует новые поля

- [ ] **Frontend** (`frontend/web/src/components/ConfidenceAndRisk.tsx` — новый компонент)
  - Две независимые визуализации:
    1. Confidence bar (0.0–1.0) с breakdown: "Model (0.8, high) | Experience (0.4, low) | Tools (0.6, medium) | Reflection (0.75, high)"
    2. Risk badge: "🔴 HIGH RISK" с пояснением "git.push, shell.execute"
  - При ask: дать причину breakdown'ом (например "Experience low, Tools uncertain")
  - При force-approve для high-risk: модальное окно с обязательным комментарием + checkbox подтверждения + audit log

- [ ] **Производительность & кэширование** (`crates/agent-runtime/src/agent_loop/confidence_cache.rs` — новый)
  - In-memory кэш с TTL 60 секунд: `{task_id -> confidence_score, risk_level}`
  - Batch-query: для списка tools, одним SELECT'ом получить статистику всех за раз
  - Индексы: `tool_execution_stats(tool_name, created_at)`, `reflection_events(task_id, created_at)`
  - Timeout на retrieval (max 500ms) → fallback к ask при таймауте

### ✅ Тесты

- [ ] **Unit tests** (`crates/agent-runtime/tests/confidence_gate.rs`)
  - `test_high_impact_requires_approval` — high-risk всегда требует approval, независимо от confidence
  - `test_weights_normalize_to_one` — сумма весов = 1.0 при любых входах
  - `test_model_confidence_low_reliability_penalty` — низкая reliability штрафует итоговый score
  - `test_missing_signals_trigger_ask` — при 2+ отсутствующих сигналах → ask()
  - `test_tool_success_rate_smoothing` — smoothing работает корректно, min_history limit respected
  - `test_experience_alignment_with_min_k` — alignment = 0.5 если <2 похожих playbooks
  - `test_reflection_confidence_decay` — старые ревизии забываются
  - `test_fallback_chain` — providercapability detection + fallback по цепочке
  - `test_risk_level_by_tools` — high-risk tools корректно определяются
  - `test_thresholds_risk_aware` — пороги меняются в зависимости от risk_level

- [ ] **Integration tests** (`crates/server/tests/confidence_integration.rs`)
  - End-to-end: создать task с mixed confidence + high-risk → проверить ask/require decision
  - Миграции: `migrations/0035` вверх/вниз, дефолты для старых записей
  - Audit trail: проверить `confidence_audit_log` записывается при каждом ask/proceed
  - WS-события: `agent.confidence` приходит с корректной версией schema
  - Reflection integration: пересмотр плана правильно обновляет `confidence_delta`
  - Memory integration: `model_confidence_at_creation` сохраняется и используется при retrieval
  - Config: `EVOHIME_CONFIDENCE_GATE_ENABLED=0` полностью отключает логику, fallback к старому

- [ ] **Frontend tests** (`frontend/web/src/components/__tests__/ConfidenceAndRisk.test.tsx`)
  - Компонент отображает breakdown для каждого сигнала
  - High-risk показывает warning с красным бейджем
  - Force-approve требует комментарий для high-risk
  - Missing signals показываются явно в UI

- [ ] **Tests**
  - Unit: `crates/agent-runtime/tests/confidence_gate.rs`
    - Test extraction: model_confidence parsing from thinking tokens
    - Test experience alignment: retrieval scores
    - Test tool_success_rate: stat aggregation
    - Test reflection decay: revision tracking
    - Test aggregation formula: edge cases (all-high, all-low, mixed)
  - Integration: `crates/server/tests/confidence_integration.rs`
    - End-to-end: task with mixed confidence signals → correct ask/proceed decision
    - Regression: 8.2 reflection loop still works with new confidence signals

- [ ] **Documentation** (`docs/features/confidence-ask-gate.md`)
  - Explain each signal and why it matters
  - Configuration knobs: `EVOHIME_CONFIDENCE_THRESHOLDS` (json: {proceed, ask, require})
  - Opt-out: `EVOHIME_CONFIDENCE_GATE_ENABLED=0` (fallback to old uncertainty logic)

- [ ] **CI/commit**
  - Schema migration passes
  - Rust builds, clippy clean
  - Frontend typecheck/build passes
  - All tests green
  - Evidence: commit message references task 8.4

## Architecture

```text
ReAct Loop (after tool plan generated)
  │
  ├─ [1] Risk Engine (risk_engine.rs)
  │    └─ analyze planned steps → risk_level: {None, Low, Medium, High}
  │
  ├─ [2] Confidence Engine (confidence_gate.rs)
  │    ├─ model_confidence.rs: extract model conf + reliability {high/medium/low/very_low}
  │    ├─ experience_align.rs: retrieval top-3 playbooks → weighted mean alignment
  │    ├─ tool_metrics.rs: query smoothed success rate per tool (Beta-binomial prior)
  │    ├─ reflection_confidence.rs: read revision_type count from reflection_events
  │    └─ aggregate: confidence = 0.35*model + 0.25*exp + 0.25*tool + 0.15*reflection
  │       (with reliability penalties: low→-0.1, very_low→-0.15)
  │
  ├─ [3] Ask Policy (ask_policy.rs)
  │    ├─ if risk >= High AND confidence < 0.85: require_approval()
  │    ├─ elif confidence >= thresholds[risk].proceed: proceed()
  │    ├─ elif confidence >= thresholds[risk].ask: ask()
  │    └─ else: require_approval()
  │
  ├─ [4] Audit & Events
  │    ├─ write confidence_audit_log {confidence, risk, breakdown, missing_signals, decision}
  │    └─ emit WS agent.confidence {version, score, risk, breakdown, reliability, recommendation, missing_signals}
  │
  └─ [5] UI
       ├─ ConfidenceAndRisk component (two independent visuals: bar + risk badge)
       ├─ Breakdown tooltip per signal with reliability
       └─ force-approve modal (high-risk only, requires comment)
```

**Версионирование:** все структуры схемы помечены `version: "1"`, миграции и код заготовлены под будущие версии.

## Файлы, которые будут изменены / созданы

**Backend — ядро (обязательные)**
- `migrations/0035_confidence_signals.sql` — `tool_execution_stats`, `memory_items.model_confidence_at_creation`, `reflection_events.revision_type/confidence_delta`, `confidence_audit_log`
- `crates/agent-runtime/src/agent_loop/risk_engine.rs` (новый) — `determine_risk_level(plan_steps) -> RiskLevel`
- `crates/agent-runtime/src/agent_loop/model_confidence.rs` (новый) — extraction с reliability levels
- `crates/agent-runtime/src/agent_loop/confidence_gate.rs` (новый) — агрегация 4 сигналов, формула, weighting
- `crates/agent-runtime/src/agent_loop/ask_policy.rs` (новый) — decision logic с risk-aware thresholds
- `crates/agent-runtime/src/agent_loop/confidence_cache.rs` (новый) — in-memory cache TTL + batch-query optimization
- `crates/agent-runtime/src/agent_loop/react.rs` (изменить) — вызов risk_engine + confidence_gate перед approval точкой
- `crates/agent-runtime/src/agent_loop/mod.rs` — pub mod all новых модулей

**Backend — хранение**
- `crates/storage/src/tool_metrics.rs` (новый) — DAO для `tool_execution_stats` CRUD + smoothed query
- `crates/storage/src/lib.rs` — pub mod tool_metrics
- `crates/storage/src/reflection.rs` (изменить) — добавить revision_type/confidence_delta persist
- `crates/storage/src/confidence_audit.rs` (новый) — DAO для `confidence_audit_log` insert

**Backend — API & events**
- `crates/server/src/agent_loop_api.rs` (изменить) — emit `agent.confidence` WS-событие после compute_confidence()
- `crates/server/src/settings_api.rs` (изменить) — GET/PUT `/api/settings/confidence-thresholds` для конфига
- `crates/protocol/schema/evohime.protocol.schema.json` — добавить `AgentConfidence` с полями версии, breakdown, reliability, missing_signals

**Backend — конфигурация**
- `.env.example` — добавить `EVOHIME_CONFIDENCE_*` переменные с дефолтами
- `crates/server/src/config.rs` (изменить) — парсить JSON thresholds и флаги

**Frontend — компоненты**
- `frontend/web/src/components/ConfidenceAndRisk.tsx` (новый) — две независимые визуализации: confidence bar + risk badge
- `frontend/web/src/components/ConfidenceBreakdown.tsx` (новый) — tooltip с деталями каждого сигнала + reliability indicators
- `frontend/web/src/components/ForceApproveModal.tsx` (новый) — модальное окно для force-approve high-risk (comment + checkbox)
- `frontend/web/src/panels/ChatPanel.tsx` (изменить) — интегрировать ConfidenceAndRisk и ForceApproveModal в approval flow
- `frontend/web/src/hooks/useConfidenceSignal.ts` (новый) — hook для обработки `agent.confidence` WS-событий
- `frontend/web/src/protocol.ts` — re-export `AgentConfidence` и обновить types

**Frontend — тесты**
- `frontend/web/src/components/__tests__/ConfidenceAndRisk.test.tsx` (новый)
- `frontend/web/src/components/__tests__/ForceApproveModal.test.tsx` (новый)

**Backend — тесты**
- `crates/agent-runtime/tests/confidence_gate.rs` (новый) — unit tests for all 5 signals
- `crates/agent-runtime/tests/ask_policy.rs` (новый) — risk-aware threshold tests
- `crates/storage/tests/tool_metrics.rs` (новый) — smoothing, min_history, batch-query tests
- `crates/server/tests/confidence_integration.rs` (новый) — E2E: task → compute → ask/proceed → audit_log

**Docs**
- `docs/features/confidence-ask-gate.md` (новый) — complete spec с примерами, formulas, config
- `docs/roadmap.md` (изменить) — update 8.4 evidence и ссылку на feature doc
- `docs/superpowers/specs/2026-08-03-confidence-ask-gate-design.md` (новый) — детальный дизайн, trade-offs, future extensions

## Статус реализации: ✅ ЗАВЕРШЕНО

### Завершено:
✅ **DB Schema** — миграция 0039 с tool_execution_stats, confidence_audit_log, reflection/memory расширения  
✅ **Storage DAOs** — tool_metrics.rs, confidence_audit.rs с CRUD операциями и batch queries  
✅ **Confidence Engine** — risk_engine, model_confidence, confidence_gate, ask_policy модули  
✅ **Compute Helper** — confidence_compute.rs для высокоуровневой интеграции  
✅ **Protocol** — AgentConfidence WS event с версионированием в schema и Rust enums  
✅ **API Endpoints** — GET/PUT confidence-thresholds, audit logs по task/session  
✅ **Frontend Components** — ConfidenceAndRisk (bar+breakdown+risk-badge), ForceApproveModal  
✅ **Frontend Styling** — CSS с dark-mode, a11y, responsive layout  
✅ **Integration Tests** — confidence_gate_integration.rs с 7 тестами  
✅ **Feature Docs** — docs/features/confidence-ask-gate.md (366 строк, полная spec)  
✅ **Compilation** — весь workspace компилируется успешно  

### Коммиты:
1. `86514d6` — Infrastructure (migrations, DAOs, engine modules)
2. `dfbd903` — Compilation fixes (tool_metrics, storage)  
3. `0db73ca` — Protocol event (AgentConfidence)
4. `fc49724` — API endpoints и routes
5. `d43da7f` — Compute helpers, frontend, tests
6. `0464df8` — Documentation

### Оставлено для Post-MVP:
- [ ] Runtime integration (вызов compute_confidence в ReAct loop)
- [ ] UI модал интеграция в approval flow
- [ ] Settings persistence в БД (сейчас env-only)
- [ ] Calibration dashboard
- [ ] Auto-tuning weights
- [ ] A/B testing framework  

## Обязательные элементы перед кодированием

1. **Code review плана** у двух senior разработчиков на предмет:
   - Корректность формул (веса, normalization, clamp ranges)
   - Обработка edge cases (empty memory, no tool history, >1 missing signal)
   - Performance: batch-query, indices, cache TTL
   - Backward-compatibility миграций

2. **Дизайн-ревью UI** (ConfidenceAndRisk, ForceApproveModal) с UX/a11y фокусом:
   - Accessibility: ARIA labels, keyboard navigation, screen reader testing
   - Dark mode, responsive mobile layout
   - Tooltip/explanation clarity

3. **Config validation script** — перед deploy убедиться, что JSON thresholds well-formed и reasonable (0 <= proceed <= ask <= require <= 1.0)

## Архитектурные замечания

- Это не блокирует 8.1 (автоматический перезапуск планировщика) — ортогональные фичи
- 8.2 (self-reflection) переиспользуется, не переписывается — revision_type/confidence_delta — новые колонки, а не рефакторинг
- Experience alignment переиспользует existing retrieval из 6.19, не требует новых embeddings
- Model confidence detector работает при поддержке провайдером (graceful fallback если нет)
- Risk engine — простой синтаксический анализ plan steps, не требует AST парсинга
- Tool metrics работают на базе простой статистики в PG, не требует ML или внешних сервисов
- Все пороги конфигурируемы (JSON), no hardcodes — будущие итерации могут туюнить без кода

## Оценка трудозатрат

**M (3–7 дней)** сохраняется при условии:
- Веса/формулы заморожены после этого плана (калибровка через конфиг post-deploy)
- Frontend компоненты переиспользуют существующие patterns (стили, modal helpers)
- Миграция БД просто — только новые колонки, no schema reshaping
- Интеграционные тесты на базе существующего harness из 7.84
