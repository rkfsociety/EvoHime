# Этап 22.1 — diagnostics и recovery UX

Статус: готов к реализации после ревью обзора 22.

## Цель и граница

Свести recovery, repair, update и Core diagnostics в понятную bounded
projection. Пользователь должен видеть причину, текущую фазу, correlation/
sequence и разрешённое следующее действие, не получая raw prompts, tool output,
workspace files, provider output или secrets.

## Зависимости

Блокирующие: [`../architecture.md`](../architecture.md),
[`../current-state.md`](../current-state.md), `desktop/evohime-electron/src/main/diagnostics/`,
`desktop/evohime-electron/src/main/recovery.ts`, `RecoveryBanner.tsx`,
`RepairService`, Core recovery events и существующий diagnostic bundle v1.

Опциональные: дополнительный Core Doctor detail level и GitHub check-run API;
без них summary projection должна оставаться работоспособной.

## Работы

1. Зафиксировать единую таблицу typed states/reason codes для Core recovery,
   shell repair и updater health; устранить неоднозначные состояния и stale
   responses.
2. Проверить, что diagnostic bundle собирается только main-процессом, имеет
   размер/event-tail/log bounds, стабильную schema version и redaction перед
   записью; UI должен получать только save result и безопасный summary.
3. Согласовать RecoveryBanner, OperationsPanel и UpdateGate с одной projection:
   recovery blocked/unknown outcome, retryable failure, waiting approval,
   health timeout и successful recovery не должны смешиваться.
4. Добавить focused tests на неизвестные reason codes, sequence gaps,
   повторную загрузку состояния, redaction и отказ записи bundle; добавить
   real-Core/fixture coverage там, где она доступна.

## Критерии приёмки

- любой recovery outcome отображается typed-состоянием и безопасным действием;
- повторное событие или устаревший ответ не меняет более новое состояние;
- bundle не содержит абсолютных путей, credentials, prompts, raw output или
  PII и всегда укладывается в установленный лимит;
- отмена, approval и repair остаются отдельными действиями пользователя;
- Electron protocol/typecheck/tests, Rust recovery tests и release evidence
  gate проходят.

## Не входит

Автоматический repair, commit, push, restart, удалённая диагностика и изменение
Core recovery semantics без отдельного контракта.
