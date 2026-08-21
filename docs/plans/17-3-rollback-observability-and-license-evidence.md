# План 17.3. Откат, диагностика и release evidence

## Цель

Зафиксировать, как доказать безопасную поставку, восстановление и удаление
опционального компонента без утраты состояния или аудита.

## Изменения

- Для каждой миграции и внешнего backend описать backup, rollback, disable,
  cleanup staging/temp data и поведение после supervisor/Core restart.
- Собирать bounded redacted diagnostics: correlation/run id, schema/backend
  version, typed outcome, budget и recovery state без secrets/raw media.
- Сопоставить deterministic fixture/replay artifacts с commit, contract version,
  environment и expected outcome.
- Вести packaging, license/attribution, privacy, egress и maintenance inventory;
  явно отмечать optional dependencies и их fallback.
- Определить retention и export policy для audit, history, benchmark и
  diagnostic artifacts.

## Проверки

- rollback после успешной и частично завершённой миграции;
- crash/restart, disabled backend, missing package и corrupted staging;
- redaction/retention/export review;
- воспроизводимость fixture и отсутствие credentials в artifacts;
- license/egress/privacy checklist на clean release package.

## Готово, когда

Любая поставка имеет evidence bundle, из которого видно что изменилось,
как восстановиться, какие зависимости разрешены и какие данные удаляются;
diagnostics полезны, но не раскрывают секреты.

