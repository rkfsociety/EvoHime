# EvoHime — финальный release audit

Статус audit: `TECHNICAL_GATES_PASS / RELEASE_BLOCKED` — блокирует только
внешнее certificate-backed code-signing evidence.

Свежий прогон выполняется `scripts/final-release-audit.tests.ps1`. Он проверяет
Rust Core/SQLite/IPC tests, rustfmt, automation boundary, backup/restore and
redaction evidence gates, а также Electron protocol и typecheck. Package
startup/fault/installer smoke остаются отдельными Windows CI gates из
`.github/workflows/windows.yml`.

## Остаточные release blockers

- `O-SIGN-01`: workflow и локальный signing gate реализованы, но в текущем
  checkout нет `signtool.exe` и сертификата, поэтому certificate identity,
  timestamp и Authenticode evidence ещё не получены.

Закрытые решения текущего плана: `O-AUTO-01` (scheduler/IPC gates),
`O-AUTO-02` (transactional archive/restore и retention evidence) и `O-LIC-01`
(locked Cargo/npm inventory gate). Они больше не являются основанием для
`RELEASE_BLOCKED`.

## Подтверждённые границы

Планы 01–17 перенесены в canonical docs и удалены из каталога только после
focused checks и task-only commits. Optional browser/voice/vision adapters
деградируют в typed unsupported и не расширяют base runtime.
