# Этап 22.3 — compatibility и release hardening

Статус: ревью пройден, готов к реализации после 22.1; security-зависимые
изменения учитывать из 22.2.

## Цель и граница

Сделать release evidence воспроизводимым и поддерживать фактическую матрицу
Windows 10/11 для Electron, Core, supervisor, installer, update/rollback и
compatibility shell. ARM64 и Insider проверяются как optional informative
runs, не меняющие обязательный Windows x64 release.

## Зависимости

Блокирующие: `.github/workflows/windows.yml:35-74`, native package scripts,
`scripts/final-release-audit.tests.ps1`, `scripts/release-evidence.tests.ps1`,
`scripts/documentation.tests.ps1` (создаётся этапом), C# compatibility tests,
`installer/release-notes.md` и [`../release-evidence.md`](../release-evidence.md).

Опциональные: доступные ARM64/Insider runners и дополнительные GitHub CI API;
при их отсутствии x64 gates и evidence должны оставаться полными.

## Работы

1. Создать `scripts/documentation.tests.ps1`, который проверяет все tracked
   Markdown relative links и запрещает ссылки на удалённые планы/архивы;
   подключить его к Windows CI для commits, затрагивающих docs/plans/README.
2. Инвентаризировать обязательные gates и их evidence: protocol/typecheck,
   Rust tests/clippy/fmt, package startup, single-instance/Job Object,
   installer, upgrade, health handshake, rollback и compatibility IPC.
3. Устранить drift между локальными scripts, workflow и release documents;
   каждый gate должен иметь bounded timeout, понятный failure artifact и
   redacted output.
4. Добавить matrix checks для Windows 10 2004+ и Windows 11 без заявлений о
   неподтверждённой поддержке; отдельно документировать optional adapter
   `backend_unavailable` и signing вне текущего scope.
5. При наличии runner добавить ARM64/Insider informative job с явным
   `continue-on-error` только для optional job и отдельным статусом evidence;
   optional failure не должен маскировать x64 failure или превращаться в pass.

## Критерии приёмки

- обязательные x64 Windows release gates не имеют stale paths или удалённых
  plan references;
- documentation gate обнаруживает broken relative links и удалённые plan
  references до публикации release;
- CI evidence содержит commit, gate/test ID, outcome и redaction status, но не
  credentials, raw output, PII или абсолютные пользовательские пути;
- compatibility tests подтверждают additive IPC и graceful unsupported
  optional adapters;
- optional ARM64/Insider результат виден отдельно и не изменяет release
  decision;
- `final-release-audit.tests.ps1`, native package smoke и Windows CI проходят.

## Не входит

Перевод базового продукта на ARM64, code signing, новый release channel,
обязательный cloud service или возврат legacy web/PostgreSQL runtime.

## Откат и инвалидация

Documentation gate не меняет runtime и может быть отключён только удалением
его job/step с одновременным изменением release evidence. Optional ARM64/Insider
job не влияет на обязательный x64 decision; его результат хранится отдельно.
