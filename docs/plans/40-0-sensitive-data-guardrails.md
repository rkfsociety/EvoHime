# План 40.0 — Sensitive Data Guardrails: PII/secret detection и streaming redaction на model/tool boundaries

Статус: предложено по [issue #20](https://github.com/rkfsociety/EvoHime/issues/20). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime системный **Sensitive Data Guardrail Pipeline** для обнаружения и обработки PII/секретов на границах model/tool/renderer/trace до того, как данные уйдут во внешний provider, лог, stream или другой менее доверенный контекст.

Это не замена существующей классификации `Public / Sensitive / Secret` и secret storage. Новый слой должен работать с **содержимым**, которое само по себе может содержать чувствительные фрагменты даже если источник не был заранее помечен как Secret.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/sensitive-data-guardrails.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries.
- План 92.0 — Privacy & Telemetry Governance: consent, typed analytics events и sensitive-data boundaries.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Есть versioned SensitiveDataRule.
- [ ] Guardrails применяются на model/tool/stream/trace boundaries.
- [ ] Поддерживаются redact/mask/hash/block.
- [ ] Structured payload обрабатывается рекурсивно.
- [ ] Streaming detector ловит patterns между chunks.
- [ ] Provider destination может менять policy.
- [ ] Trace может существовать без raw payload.
- [ ] Raw local authoritative data и redacted projection явно разделены.

## Ограничения и non-goals

- полноценный enterprise DLP scanner;
- OCR всех изображений ради поиска PII;
- отправка sensitive content стороннему классификатору для redaction;
- автоматическое удаление локальных пользовательских файлов;
- замена OS credential storage;
- гарантированное распознавание любых возможных секретов эвристикой.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#20 Sensitive Data Guardrails: PII/secret detection и streaming redaction на model/tool boundaries](https://github.com/rkfsociety/EvoHime/issues/20)
