# Этап 22.2 — credentials и backup hardening

Статус: ревью пройден, готов к реализации после 22.1.

## Цель и граница

Проверить и усилить полный lifecycle provider credentials и Core-owned SQLite
backup/restore: создание, замена, удаление, сбой, восстановление и диагностика.
Секреты не должны попадать в renderer, SQLite backup, logs, diagnostic bundle,
Git или provider error output.

## Зависимости

Блокирующие: `desktop/evohime-electron/src/main/provider-store.ts:158-294`,
`crates/evohime-local-storage/src/backup.rs`, supervisor environment handoff,
DPAPI/safeStorage contract, existing receipt/policy boundaries и
[`../release-evidence.md`](../release-evidence.md).

Опциональные: миграция legacy credential records и дополнительные Windows
Credential Manager diagnostics; базовый путь должен работать без них.

## Работы

1. Добавить отдельный persisted-ciphertext limit и повторно валидировать его
   при чтении; `ProviderStore.save` должен сам нормализовать API key, даже если
   IPC boundary будет обойден тестовым/будущим caller.
2. Сделать `ProviderStore.write` атомарной и crash-safe: ограниченный временный
   файл, mode 600, flush/fsync, rename и cleanup temp при любой ошибке.
3. Провести матрицу provider profile lifecycle: first save, overwrite,
   clear, interrupted write, decrypt failure, unavailable safeStorage и
   restart handoff. Для каждого исхода определить cleanup и redacted status.
4. Проверить identity/ACL/permissions всех persisted settings и runtime
   directories; запретить accidental legacy API-key retention и включение
   credential material в backup/restore preview, logs и evidence.
5. Проверить backup manifest, checksum, schema compatibility, safety backup,
   cancellation, partial restore и retention. Добавить negative tests на
   подмену пути, checksum, schema и попытку восстановить provider secrets.
6. Обновить recovery/diagnostic projection так, чтобы она сообщала только
   typed failure и remediation, без значения секрета или чувствительных путей.

## Критерии приёмки

- provider credentials доступны только выбранному provider через supervisor;
- после любой ошибки временные секретные материалы удаляются или остаются
  недоступными для чтения;
- oversized/invalid persisted ciphertext трактуется как отсутствующий ключ и
  не передаётся в decrypt/environment;
- backup/restore атомарны, checksum- и approval-protected, cancellation-safe;
- backup preview явно сообщает, что provider secrets не восстанавливаются;
- focused Rust/Electron/Windows compatibility tests и release evidence gate
  проходят без credentials в логах и артефактах.

## Не входит

Облачное хранилище секретов, автоматическая синхронизация профилей, изменение
модели доверия Core/supervisor или перенос secret ownership в renderer.

## Откат и инвалидация

Формат encrypted profile остаётся `version: 1`; новый лимит только отбрасывает
повреждённые/чрезмерные записи как unconfigured. При сбое записи старый файл
остаётся на месте, временный файл удаляется. Backup schema и safety-backup
transaction не меняются без отдельной migration/rollback записи.
