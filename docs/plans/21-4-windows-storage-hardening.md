# План 21.4 — Credentials, backup/restore и Windows upgrade hardening

Статус: draft, зависит от утверждения обзора 21-0.

## Цель

Проверить и улучшить пользовательский контур credentials, backup/restore и
установочных переходов на поддержанных Windows 10 2004+ и Windows 11 без
ослабления DPAPI, sandbox или rollback guarantees.

## Зависимости

### Блокирующие

- обзор 21-0;
- текущие Electron `safeStorage`/DPAPI contracts;
- Core-owned SQLite backup/restore с checksum, cancel и rollback;
- installer, transaction worker, health-marker и Windows CI.

### Опциональные

- Credential Manager: допустим только как локальная реализация с тем же
  redacted summary contract;
- Windows 11-only diagnostics: typed unsupported на Windows 10.

## Работы

- провести threat/UX review хранения provider credentials и отказов DPAPI;
- зафиксировать DPAPI как канонический текущий контракт; Credential Manager рассматривать только отдельным decision gate, без обязательной реализации в этом этапе;
- довести UI backup/restore до ясных preview, progress, cancel, checksum,
  safety backup, rollback и audit states;
- сформировать Windows matrix для install, upgrade, locked files, single
  instance, Job Object, health timeout и recovery незавершённой транзакции;
- собирать release evidence с OS version, architecture, build mode и commit;
- проверить, что документация не обещает signing, если он остаётся вне scope.

## Acceptance gates

- Rust storage и Electron credential tests;
- backup/restore cancellation, checksum, corruption и rollback tests;
- Windows 10/11 installer and upgrade smoke;
- no secret exposure in UI, logs, evidence or backups;
- green Rust/Electron/package/Windows acceptance gates.

## Результат

Пользователь понимает состояние данных и обновления, а установка либо проходит
health-check, либо автоматически возвращается к рабочей версии.
