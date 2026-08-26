# План 22 — reliability и security hardening

Статус: подготовлен к ревью и реализации, 26 августа 2026 года.

## Цель

Укрепить уже реализованный локальный Windows-клиент: сделать диагностику и
восстановление понятнее пользователю, усилить защиту credentials и backup/
restore, а также сохранить воспроизводимость Windows release gates. Core
остаётся владельцем состояния, политик, эффектов и SQLite; renderer только
показывает projection и отправляет явно разрешённые команды.

## Зависимости

Блокирующие: текущая архитектура Electron + Rust Core + supervisor, планы
recovery/update, authenticated desktop IPC, существующие backup/restore и
release-evidence gates.

Опциональные: GitHub API для расширенного CI evidence, ARM64/Insider runner и
внешние optional adapters. Их отсутствие не должно блокировать Windows x64
package или переводить систему в permissive fallback.

## Порядок этапов

1. [`22-1-diagnostics-and-recovery-ux.md`](22-1-diagnostics-and-recovery-ux.md)
   — единая и проверяемая диагностика/recovery projection.
2. [`22-2-credentials-and-backup-hardening.md`](22-2-credentials-and-backup-hardening.md)
   — credential lifecycle и backup/restore hardening.
3. [`22-3-compatibility-and-release-hardening.md`](22-3-compatibility-and-release-hardening.md)
   — Windows compatibility, reproducible release evidence и optional ARM64.

Этап 22.2 зависит от 22.1 только в части общего diagnostic evidence; этап 22.3
может стартовать после 22.1, но его release-gate изменения должны учитывать
контракты 22.2. Никакой этап не возвращает web runtime, public HTTP, внешний
Node/Python runtime или автоматические repair/push/restart действия.

## Общие критерии готовности

- изменения имеют bounded input/output, cancellation и redacted diagnostics;
- renderer не получает secrets и не принимает policy/effect decisions;
- добавлены focused unit/contract/integration tests и обновлены acceptance gates;
- `current-state.md`, `architecture.md`, `decision-register.md` и
  `release-evidence.md` обновлены только по фактически реализованному;
- проходят `cargo fmt`, строгий `clippy`, соответствующие Rust/Electron/C# и
  Windows package checks, `git diff --check`.
