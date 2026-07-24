# Continual learning wave 2: эскалация повторов + retrieval-приоритизация уроков

## Цель

Закрыть остаток `7.103` (online continual learning): два пункта, ранее явно вынесенные за скобки wave 1 —

1. **Эскалация confidence/importance при повторении** failure-паттерна: если один и тот же `failure_pattern`/`verification_rule` возникает снова (дубликат при admit), это уже не гипотеза, а подтверждённый факт о провале — вес урока должен расти.
2. **Retrieval-приоритизация уроков**: `failure_pattern`/`verification_rule` должны всплывать выше прочих experience-записей (`success_pattern`, `playbook`) при прочих равных, так как предотвращение повторной ошибки ценнее общей рекомендации.

Оба пункта — backend-only, без изменений протокола/схемы БД/фронтенда (см. решение ниже).

## Пересмотр старого решения

Wave 1 сознательно не трогала повторы ("Реализационные границы: вне волны — автоматическая эскалация confidence при повторении паттерна, retrieval-приоритизация"). Инвариант wave 1 остаётся неизменным: **confidence кандидата из провала никогда не пересекает `AUTO_PROMOTE_CONFIDENCE` (0.7)** — auto-promote из провала невозможен by construction, вне зависимости от числа повторов. Эскалация усиливает вес урока (importance, retrieval-ранг), но не открывает путь к auto-promote.

## Реализация

### 1. Эскалация повторов (`crates/memory`)

Переиспользуем существующий feedback-пайплайн (`crates/memory/src/feedback.rs` + `feedback_service.rs`, тот же механизм, что уже применяется для `helpful`/`harmful`/`corrected`) — без новой миграции. Число повторов при необходимости в будущем можно восстановить из `memory_feedback_events` (уже пишется на каждый `apply_one`), отдельный счётчик-колонка не нужен.

- `feedback.rs`: новый вариант `FeedbackSignal::Repeated` ("repeated"). `apply_feedback_signal`:
  - `next_importance = clamp01(importance + FAILURE_REPEAT_IMPORTANCE_BUMP)` (bump 0.1, без верхнего капа кроме стандартного clamp 0..1 — `decide_gate` importance не читает, рост безопасен и работает на retrieval-приоритет).
  - `next_confidence = clamp01(before + FAILURE_REPEAT_CONFIDENCE_BUMP).min(extract::FAILURE_CONFIDENCE_CAP)` (bump 0.05, жёсткий кап 0.6 — существующая константа из wave 1, импортируется, не дублируется).
  - `next_status`: не меняется (остаётся как есть).
- `feedback_service.rs`: `record_memory_repeated(pool, memory_id, task_id)` через существующий `apply_one`.
- `service.rs` (`admit_memory_item`): на ветке `Evaluation::Duplicate { existing_id }` — если у найденного дубликата (уже есть в `existing: &[ExistingMemory]`, загружены до `evaluate`) `scope == Experience`, `kind ∈ {FailurePattern, VerificationRule}` и `status == Candidate`, вызвать `record_memory_repeated(pool, existing_id, prepared.item.source_task_id)`. Во всех остальных случаях (обычные факты, либо дубликат уже `Active`/`Rejected`/`Conflict` — операторское решение уважается, не трогаем) — поведение не меняется.
- `AdmitOutcome`/`GateDecision`/`gate_after_admit` не меняются: эскалация — чистый побочный эффект внутри `admit_memory_item`, никаких новых `Ask`/событий, вызывающий код (`persist_structured_memory`) не меняется вовсе.

### 2. Retrieval-приоритизация (`crates/memory/src/retrieve.rs`)

В `score_item`: сейчас все experience-kind (`SuccessPattern | FailurePattern | VerificationRule | Playbook`) получают одинаковый `+0.3`. Добавляется дополнительный `+0.2` конкретно для `FailurePattern | VerificationRule` (итого `+0.5` против `+0.3` у `SuccessPattern`/`Playbook`), так что при равной лексической/семантической релевантности урок о провале ранжируется выше. В сочетании с ростом `importance` от эскалации (`score += item.importance` уже в формуле) — оба пункта roadmap закрываются одним изменением скоринга плюс эскалацией.

## Поведение и ошибки

- Эскалация не создаёт новых строк, не меняет статус, не эмитит события — тихий backend-эффект.
- Если дубликат находится у записи не в статусе `Candidate` (уже принята/отклонена/в конфликте оператором) — эскалация пропускается: решение оператора не переигрывается.
- Confidence физически не может превысить 0.6 у failure-lane записей ни при каком числе повторов — инвариант wave 1 сохраняется.
- Non-experience и non-failure-kind дубликаты (обычные facts/preferences) ведут себя как раньше — без изменений.

## Границы

Вне волны: явный счётчик повторов в UI/протоколе (сознательно отложено — можно достать из `memory_feedback_events` при необходимости), периодическая консолидация уроков, эскалация success_pattern/playbook (не relevant — они не про провалы).

## Проверка

- `feedback.rs`: unit-тест `repeated_signal_bumps_importance_and_caps_confidence_at_0_6` — confidence из 0.55 → ≤0.6 после `Repeated`; из уже-0.6 остаётся 0.6 (не растёт дальше); importance растёт без верхнего капа кроме 1.0.
- `service.rs`: тест на `admit_memory_item` — второй admit того же failure_pattern (тот же content, scope=Experience) даёт `Duplicate`, а исходная запись после этого имеет более высокую importance/confidence (через прямое чтение строки); дубликат обычного `Fact` не вызывает эскалацию; дубликат `FailurePattern` уже в статусе `Rejected` не меняет запись.
- `retrieve.rs`: тест — `FailurePattern`/`VerificationRule` ранжируются выше `SuccessPattern`/`Playbook` при равных importance/query-overlap.
- Полный `cargo test --workspace`, Clippy.

## Критерий готовности

Повторный провал с уже известным `failure_pattern`/`verification_rule` (тот же контент, admit-дубликат) поднимает confidence (капом ≤0.6) и importance существующей experience-записи без новых событий/строк; при retrieval эта запись и вообще все `failure_pattern`/`verification_rule` ранжируются выше `success_pattern`/`playbook` при равной релевантности. Auto-promote из провала по-прежнему невозможен ни при каком числе повторов.
