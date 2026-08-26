# Этап 22.1 — diagnostics и recovery UX

Статус: ревью пройден, готов к реализации.

## Цель и граница

Свести recovery, repair, update и Core diagnostics в понятную bounded
projection. Пользователь должен видеть причину, текущую фазу, correlation/
sequence и разрешённое следующее действие, не получая raw prompts, tool output,
workspace files, provider output или secrets.

## Зависимости

Блокирующие: [`../architecture.md`](../architecture.md),
[`../current-state.md`](../current-state.md),
`desktop/evohime-electron/src/main/diagnostics/bundle.ts:33-60`,
`desktop/evohime-electron/src/main/recovery.ts`,
`desktop/evohime-electron/src/renderer/src/recovery-state.ts`,
`desktop/evohime-electron/src/renderer/src/RecoveryBanner.tsx:101-111`,
`RepairService`, Core recovery events и существующий diagnostic bundle v1.

Опциональные: дополнительный Core Doctor detail level и GitHub check-run API;
без них summary projection должна оставаться работоспособной.

## Работы

1. В `bundle.ts` читать только bounded tail каждого log file и ограничивать
   суммарное число файлов/строк до redaction; при ошибке чтения пропускать
   файл, не раскрывая путь или exception.
2. В `recovery-state.ts` whitelist-ить и ограничить detail projection для
   renderer; terminal task IDs собирать одним проходом, чтобы approval не
   выполнял `some()` по всему event stream для каждого события.
3. Согласовать RecoveryBanner, OperationsPanel и UpdateGate с одной projection:
   recovery blocked/unknown outcome, retryable failure, waiting approval,
   health timeout и successful recovery не должны смешиваться.
4. Добавить focused tests на oversized/multiple logs, неизвестные reason codes,
   повторную загрузку состояния, redaction и отказ записи bundle; добавить
   real-Core/fixture coverage там, где она доступна.

## Критерии приёмки

- любой recovery outcome отображается typed-состоянием и безопасным действием;
- повторное событие или устаревший ответ не меняет более новое состояние;
- bundle не содержит абсолютных путей, credentials, prompts, raw output или
  PII и всегда укладывается в установленный лимит;
- чтение логов не выделяет память пропорционально размеру файла или числу
  переданных путей;
- отмена, approval и repair остаются отдельными действиями пользователя;
- Electron protocol/typecheck/tests, Rust recovery tests и release evidence
  gate проходят.

## Не входит

Автоматический repair, commit, push, restart, удалённая диагностика и изменение
Core recovery semantics без отдельного контракта.

## Откат и инвалидация

Изменения bundle/projection обратно совместимы с `v1`; при ошибке чтения или
redaction экспорт отменяется/пропускает только проблемный источник, а не
открывает raw fallback. При несовместимом payload используется безопасная
пустая projection. Новые tests должны падать, если лимиты сняты.
