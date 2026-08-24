# EvoHime — финальный release audit

Статус audit: `TECHNICAL_GATES_PASS / RELEASE_BLOCKED`.

Свежий прогон выполняется `scripts/final-release-audit.tests.ps1`. Он проверяет
Rust Core/SQLite/IPC tests, rustfmt, automation boundary, backup/restore and
redaction evidence gates, а также Electron protocol и typecheck. Package
startup/fault/installer smoke остаются отдельными Windows CI gates из
`.github/workflows/windows.yml`.

## Остаточные release blockers

- `O-AUTO-01`: scheduler timezone/missed-tick и additive automation IPC не
  wired; Core contract не объявляется automation release-green.
- `O-AUTO-02`: archive/restore transaction и retention sweep для automation
  ещё не имеют production integration evidence.
- `O-LIC-01`: `docs/licenses/` — только inventory template; точные upstream
  license texts и hashes distributed artifacts ещё нужно заполнить.
- `O-SIGN-01`: реальный code-signing pipeline/certificate evidence отсутствует;
  manifest/hash остаётся documented trust root.

## Подтверждённые границы

Планы 16.0–16.4 и 17.0–17.4 перенесены в canonical docs и удалены из каталога
только после focused checks и task-only commits. Optional
browser/voice/vision adapters деградируют typed unsupported и не расширяют
base runtime.
