# План 107.0 — Guided Calibration Sessions: iterative human feedback и versioned role guidance

Статус: предложено по [issue #87](https://github.com/rkfsociety/EvoHime/issues/87). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Guided Calibration Sessions**: явный интерактивный режим, в котором пользователь многократно даёт агенту/роли одну или несколько representative задач, оценивает первоначальный результат, формулирует корректировку, получает revised result, а EvoHime превращает подтверждённые повторяющиеся замечания в versioned guidance candidates.

Это должно дополнять Continual Refinement (#4), а не заменять его.

Разница:

```text
Continual Refinement
  -> пассивно анализирует реальные trajectories и повторяющиеся evidence

Guided Calibration
  -> пользователь намеренно запускает supervised improvement session
     и даёт explicit feedback на выбранных examples
```

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/guided_calibration_sessions.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./107-1-guided-calibration-sessions.md)
- [Этап 2 — runtime-интеграция и recovery](./107-2-guided-calibration-sessions.md)
- [Этап 3 — IPC, client projection и UI](./107-3-guided-calibration-sessions.md)
- [Этап 4 — verification, release-evidence и закрытие](./107-4-guided-calibration-sessions.md)

## Зависимости

### Блокирующие

- Реализованный Resumable Conversation Event Log v1 — канонический контракт в `docs/architecture.md`.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 46.0 — Agent Role Profiles: versioned специализация, ограничения и strategy contracts.
- План 56.0 — Artifact Handoff Registry: typed deliverables, lineage и freshness для multi-agent работы.
- План 88.0 — Approval Policy Profiles: granular standing decisions без blanket auto-approve.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- calibration не изменяет model weights;
- feedback не расширяет grants;
- session guidance scoped только session;
- raw sensitive cases redacted/controlled;
- model consolidator не имеет tools;
- activation только через Refinement pipeline;
- project feedback не становится global автоматически;
- benchmark/security regression блокирует auto-promotion;
- human feedback identity Core-owned через Human Work Item/session UI.

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

- [ ] Есть durable CalibrationSession/Iteration contracts.
- [ ] Поддержан интерактивный baseline-feedback-revision loop.
- [ ] Human feedback структурирован и имеет provenance.
- [ ] Temporary guidance не выходит за session.
- [ ] Consolidation создаёт atomic versioned guidance candidates.
- [ ] Candidates активируются только через Refinement pipeline.
- [ ] Можно сохранить/replay calibration dataset.
- [ ] Automated evals учитываются вместе с human feedback.

## Ограничения и non-goals

- fine-tuning/model weight training;
- автоматическое глобальное обучение без подтверждения;
- хранение всех production conversations как training corpus;
- скрытая вставка непрозрачного training summary во все prompts;
- калибровка security permissions/approvals;
- замена Agent Benchmark Matrix или Continual Refinement.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#87 Guided Calibration Sessions: iterative human feedback и versioned role guidance](https://github.com/rkfsociety/EvoHime/issues/87)
