# EvoHime — финальный release audit

Статус audit: `TECHNICAL_GATES_PASS / RELEASE_GREEN`.

Свежий прогон выполняется `scripts/final-release-audit.tests.ps1`. Он проверяет
Rust Core/SQLite/IPC tests, rustfmt, automation boundary, backup/restore and
redaction evidence gates, а также Electron protocol и typecheck. Package
startup/fault/installer smoke остаются отдельными Windows CI gates из
`.github/workflows/windows.yml`.

## Закрытые решения текущего плана

`O-AUTO-01` (scheduler/IPC gates),
`O-AUTO-02` (transactional archive/restore и retention evidence) и `O-LIC-01`
 (locked Cargo/npm inventory gate) закрыты. `O-SIGN-01` принят как явно
внеобъёмный для текущего цикла; release не заявляет Authenticode signature и
использует manifest/hash trust root.

`O-REPAIR-01` закрыт: пользовательский self-repair имеет отдельные кнопки для
diagnose/patch, commit, push и restart; CI failure не продвигается дальше, а
установка удерживает backup до health-marker после authenticated Core startup.

## Подтверждённые границы

Планы 01–21 перенесены в canonical docs и удалены из каталога только после
focused checks и task-only commits. Optional browser/voice/vision adapters
деградируют в typed unsupported и не расширяют base runtime.
## План 21

План 21.1–21.4 реализован в Electron shell: unified recovery projection, bounded repair/update evidence, redacted diagnostic bundle и native backup/restore file selection. На Node 22.22.2 полный Electron run дал 420 passed и 24 optional skipped; protocol/typecheck/build/bundle checks прошли. Linux Rust run дал 535/541 passed; оставшиеся 5 receipt-тестов требуют Windows key backend, а голосовой IPC-тест — Windows app catalog. Installer/upgrade, MSVC и остальные Windows acceptance gates должны быть подтверждены Windows CI.
