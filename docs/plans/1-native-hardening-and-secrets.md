# Подплан 1 — native hardening, secrets и переносимые проверки

Статус: следующий самый простой подплан
Порядок: 1 из 5
Источник: [evohime-master-plan.md](evohime-master-plan.md)

## Цель

Закрыть оставшиеся небольшие product-hardening задачи и сделать проверки воспроизводимыми на Windows 11. Этот подплан не добавляет новый агентный orchestration loop.

## Объём

- заменить POSIX-зависимые тестовые команды `true`/`false` на Windows-совместимую тестовую фикстуру;
- завершить хранение provider secrets через Credential Manager/DPAPI с ротацией и удалением старых значений;
- добавить пользовательский backup/restore SQLite с preview, progress, approval и audit;
- добавить crash-recovery UI для состояний `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL`, `FAILED`;
- закрыть security gaps: фильтрация результатов `filesystem.search`, расширенный blocklist интерпретаторов, проверка policy subject и ограничений Git remote;
- выполнить upgrade/install smoke на чистой Windows 11 22H2+.

## Порядок реализации

1. Исправить переносимость тестовых фикстур и прогнать Rust/WinUI/IPC проверки.
2. Вынести секреты из обычных настроек в Credential Manager/DPAPI; добавить тесты отсутствия секретов в logs/traces/exports.
3. Реализовать backup/restore и crash-recovery UI поверх существующего Core recovery state.
4. Закрыть search/interpreter/policy edge cases отдельными regression tests.
5. Проверить установку, обновление, rollback и recovery на чистой Windows 11.

## Критерии готовности

- `cargo test --workspace`, WinUI tests и IPC tests проходят без environment-only failures;
- provider key не хранится в plaintext settings, prompt, trace или JSONL;
- backup восстанавливается после ошибки миграции и явно показывает область восстановления;
- UI не предлагает продолжить неизвестный effect без reconciliation/approval;
- `.env` и запрещённые результаты поиска не выдаются через `filesystem.search`;
- установщик и rollback проходят на чистой reference Windows 11.

## Зависимости

Использует завершённые этапы 0b/0c и Core Doctor. Не блокирует разработку task runner, но должен быть закрыт до release hardening.
