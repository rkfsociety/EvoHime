# План 32.0 — Conversational Workflow Composer: создание и правка workflow из естественного языка

Статус: предложено по [issue #12](https://github.com/rkfsociety/EvoHime/issues/12). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Conversational Workflow Composer**: Ева получает описание желаемой автоматизации обычным языком и формирует безопасный draft существующего `workflow/v1`, который затем проходит Core validation и показывается пользователю до сохранения/запуска.

Пример:

> Сделай workflow: возьми изменения текущей ветки, отправь reviewer child, затем запусти тесты и попроси подтверждение перед commit.

Результатом должен быть не скрытый chain-of-thought и не набор немедленных tool calls, а **явное versioned workflow definition draft**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/conversational-workflow-composer.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 30.0 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей.
- План 31.0 — Visual Workflow Builder: typed canvas, validation и live runtime inspection.
- План 39.0 — Structured Response Contract: schema-first ответы модели с provider/tool fallback.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Модель предлагает структуру. Core решает, является ли она допустимым workflow.

```text
User request
  -> Composer model
  -> WorkflowDraftProposal
  -> Core normalization
  -> Registry binding
  -> Static validation
  -> Risk/permission analysis
  -> Preview
  -> User edit/accept
  -> Save new workflow version
```

Composer не получает право самостоятельно регистрировать capabilities или исполнять предложенный граф.

### Безопасность

- Composer не исполняет proposal;
- registry identities Core-owned;
- model text не используется как shell/URL identity;
- credential secrets модели не передаются;
- required approvals нельзя обойти patch-операцией;
- permission/budget subset валидируется заново после каждого edit;
- imported/generated workflows имеют одинаковый trust level до validation;
- сохраняется provenance: user request + composer model/version + resulting graph hash.

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

- [ ] Ева умеет создавать workflow draft из natural language.
- [ ] Proposal отделён от authoritative workflow contract.
- [ ] Core выполняет capability binding и validation.
- [ ] Есть iterative typed edits.
- [ ] Missing integrations/credentials показываются отдельно.
- [ ] Есть risk/side-effect preview.
- [ ] Draft можно открыть в builder и сохранить как immutable version.
- [ ] Composer не может расширить permissions или выполнить draft самовольно.

## Ограничения и non-goals

- бесконтрольная генерация новых tools;
- автоматическая установка plugins;
- запуск workflow сразу после генерации без explicit user action;
- выдача credential secrets модели;
- свободное редактирование raw workflow JSON моделью в обход Core operations;
- преобразование любой переписки в workflow без preview;
- замена обычного agent loop.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#12 Conversational Workflow Composer: создание и правка workflow из естественного языка](https://github.com/rkfsociety/EvoHime/issues/12)
