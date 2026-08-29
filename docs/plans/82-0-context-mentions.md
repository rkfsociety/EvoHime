# План 82.0 — Context Mentions: typed @references для files, folders, git, diagnostics и runtime resources

Статус: предложено по [issue #62](https://github.com/rkfsociety/EvoHime/issues/62). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime first-class **Context Mentions**: пользователь может прямо в сообщении ссылаться на конкретные Core-owned ресурсы через компактный `@...` UX, а Core разрешает ссылку в typed resource reference и формирует bounded/fresh context projection для модели.

Примеры пользовательского UX:

```text
@src/core/runtime.rs проверь этот файл
@folder:crates/evohime-core сравни архитектуру
@git:changes найди проблему в текущем diff
@git:commit:abc1234 объясни изменение
@problems исправь текущие diagnostics
@terminal:last разберись с ошибкой
@artifact:<id> проверь отчёт
```

Синтаксис может отличаться. Главное: mention не должен быть простым macro, который тупо вставляет сырой контент в prompt.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/context_mentions.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./82-1-context-mentions.md)
- [Этап 2 — runtime-интеграция и recovery](./82-2-context-mentions.md)
- [Этап 3 — IPC, client projection и UI](./82-3-context-mentions.md)
- [Этап 4 — verification, release-evidence и закрытие](./82-4-context-mentions.md)

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 75.0 — Typed Context References: адресные @refs на файлы, diff, diagnostics, terminal и artifacts.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Message text
  -> mention lexer/parser
  -> Core resolver
  -> ResourceRef
  -> permission/freshness/size checks
  -> bounded ContextProjection
  -> model context
```

Mention является **context selection**, а не capability grant.

### Безопасность

- mention не расширяет grants;
- paths canonicalized, traversal/reparse escape закрыт;
- Git refs resolve в exact object identity;
- folder expansion bounded;
- Secret/Sensitive projection соблюдается;
- raw terminal/browser payload не вставляется без guardrails;
- unknown syntax не регистрирует новый resolver;
- mention content считается untrusted model context, не system instruction;
- imported text с `@...` не должен автоматически раскрывать локальные ресурсы: resolution применяется только к explicit user-authored/typed mentions согласно source semantics.

Последний пункт критичен против prompt injection: текст из web/tool result не должен сам попросить `@../../secrets` и получить expansion.

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

- [ ] Есть typed ContextMention/ResourceRef contract.
- [ ] Поддержаны file/folder/git/diagnostics/terminal/artifact mentions.
- [ ] Explicit mentions resolve Core-side и фиксируют revision/hash.
- [ ] Folder/terminal/large resources имеют bounded projections.
- [ ] Multi-root ambiguity обрабатывается явно.
- [ ] Mention UX имеет autocomplete/chips.
- [ ] Mention не расширяет capabilities и не auto-expands из untrusted content.
- [ ] Turn provenance знает, какие exact resources были показаны модели.

## Ограничения и non-goals

- вставлять весь репозиторий по `@/`;
- arbitrary executable mention resolvers из skills;
- автоматическое раскрытие локальных файлов по mention-подобному тексту из web/tool output;
- замена Adaptive Tool Catalog;
- замена обычного context retrieval/memory;
- unrestricted URL fetching;
- хранение копии каждого mentioned file inline в conversation навсегда.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#62 Context Mentions: typed @references для files, folders, git, diagnostics и runtime resources](https://github.com/rkfsociety/EvoHime/issues/62)
