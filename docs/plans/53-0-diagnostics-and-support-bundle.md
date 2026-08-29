# План 53.0 — Diagnostics & Support Bundle: redacted health snapshot и воспроизводимый issue draft

Статус: предложено по [issue #33](https://github.com/rkfsociety/EvoHime/issues/33). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime встроенный **Diagnostics & Support Bundle**: единый Core-owned механизм, который собирает воспроизводимый снимок состояния приложения и проблемного run, автоматически применяет redaction policy и позволяет пользователю сохранить безопасный архив для диагностики или подготовки issue.

Главный принцип:

> диагностика должна быть достаточно подробной, чтобы проблему можно было воспроизвести, но не должна превращаться в автоматический экспорт секретов и пользовательских данных.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/diagnostics-and-support-bundle.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./53-1-diagnostics-and-support-bundle.md)
- [Этап 2 — runtime-интеграция и recovery](./53-2-diagnostics-and-support-bundle.md)
- [Этап 3 — IPC, client projection и UI](./53-3-diagnostics-and-support-bundle.md)
- [Этап 4 — verification, release-evidence и закрытие](./53-4-diagnostics-and-support-bundle.md)

## Зависимости

### Блокирующие

- План 25.0 — Persistent Goals: durable цели для долгих задач.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

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

- [ ] Есть versioned diagnostics bundle schema.
- [ ] Есть deterministic health checks с typed result codes.
- [ ] Bundle проходит redaction на section и final-export уровнях.
- [ ] Пользователь видит preview и redaction summary до сохранения.
- [ ] Credentials/raw secrets не экспортируются.
- [ ] Можно собрать контекст конкретного failed run.
- [ ] Генерируется локальный issue draft без автоматической публикации.

## Ограничения и non-goals

- автоматическая отправка telemetry/support bundle разработчику;
- удалённое управление машиной пользователя;
- включение полного workspace/repository в архив;
- хранение credentials ради диагностики;
- автоматический repair любых найденных проблем;
- замена обычных structured logs/metrics support bundle-ом.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#33 Diagnostics & Support Bundle: redacted health snapshot и воспроизводимый issue draft](https://github.com/rkfsociety/EvoHime/issues/33)
