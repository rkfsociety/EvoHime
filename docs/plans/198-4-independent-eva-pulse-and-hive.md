# План 198.4 — Verification, документация и release evidence

Статус: реализация и каноническая документация завершены в checkout;
CI/release evidence и формальное удаление plan-файлов ожидают push.

## Проверки

- focused Rust tests для envelope, storage, boundary, lifecycle и recovery;
- Electron tests для Hive projection, approval routing, labels и navigation
  badge;
- `git diff --check`, protocol check и документационные проверки;
- CI выполняет полный workspace/package gate; ресурсоёмкие сборки локально не
  запускаются без отдельного запроса.

## Evidence

- redacted traces для независимого запуска/остановки систем;
- proof, что EvaPulse не имеет прямого доступа к Hive coordinator;
- proof, что Hive approval отображается и разрешается в Улье;
- recovery matrix для обоих runtime и delivery outcomes.

## Документация и закрытие

- обновить `docs/architecture/hive.md` и при необходимости карту
  `docs/architecture.md`;
- обновить `docs/current-state.md`, `docs/development-plan.md` и
  `docs/release-evidence.md`;
- удалить временные plan-файлы только после подтверждения всех критериев;
- task-only commit; push только по прямому запросу Романа.
