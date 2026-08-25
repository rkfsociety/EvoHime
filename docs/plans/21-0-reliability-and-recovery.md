# План 21. Надёжность, восстановление и диагностируемость

Статус: draft, требует ревью.

## Цель

Сделать состояния approval, recovery, self-repair и обновления понятными для
пользователя и проверяемыми для сопровождения, не расширяя полномочия
Electron. После реализации пользователь должен видеть не только факт ошибки,
но и безопасное следующее действие, причину блокировки и подтверждённое
доказательство результата.

Источник текущего состояния: [`../current-state.md`](../current-state.md).
Архитектурные границы: [`../architecture.md`](../architecture.md).
Связанные направления roadmap: approval/recovery UX, CI evidence, DPAPI и
backup/restore, crash diagnostics и Windows 10/11 upgrade path.

## Не входит в план

- автоматический diagnose, patch, commit, push или restart;
- обход approval, policy, capability, sandbox или authenticated Core IPC;
- перенос состояния, SQLite или прав доступа в renderer;
- обязательные облачные сервисы, telemetry backend, внешний Node/Python runtime;
- Authenticode signing, который принят как вне scope текущего release cycle;
- изменение базового протокола без отдельного additive-контракта и
  compatibility tests.

## Зависимости

### Блокирующие

- завершённые планы 01–20 и действующие контракты Core, storage, IPC,
  supervisor и updater;
- текущий `RecoveryBanner`, `OperationsPanel`, repair FSM и transaction worker;
- durable approval/receipt/recovery состояния и существующие redacted evidence
  gates;
- Windows CI с возможностью запускать package, installer и recovery smoke.

### Опциональные

- GitHub Check Runs API: при недоступности UI показывает typed
  `ci_status_unavailable`, а repair не продвигается автоматически;
- Credential Manager: DPAPI остаётся обязательным fallback для уже поддержанного
  локального хранения, без раскрытия секрета renderer;
- Windows 11-only diagnostics: Windows 10 проверяется тем же bounded сценарием,
  а неподдержанная диагностическая функция отображается как `unsupported`;
- локальный crash dump: отсутствие dump не блокирует основной recovery evidence,
  но фиксируется как отдельное состояние.

## Граф этапов

После утверждения 21-0 этап 21.1 выполняется первым. Этапы 21.2 и 21.4 могут идти параллельно после 21-0. Этап 21.3 зависит от результата 21.1 и может выполняться параллельно с 21.2 и 21.4.
## Предлагаемая последовательность

### 21.1 — Единая модель состояний и approval UX

- провести инвентаризацию всех пользовательских состояний
  `WAITING_APPROVAL`, `RECOVERING`, `BLOCKED`, `FAILED`, `RESUMABLE` и
  `UNKNOWN_OUTCOME`;
- определить для каждого состояния bounded reason code, источник события,
  допустимые действия и условие перехода;
- привести timeline, RecoveryBanner и OperationsPanel к одной Core-owned
  проекции без вычисления FSM в renderer;
- показывать для approval structured preview, expiry, stale/call-changed и
  policy denial понятным текстом, сохраняя exact-call hash и redacted metadata;
- добавить idempotent UI-действия «повторить безопасную reconcile-проверку»,
  «отменить», «открыть evidence» и «решить approval», только если Core их
  разрешает.

### 21.2 — Evidence для self-repair и обновлений

- расширить bounded repair projection этапами diagnose, patch, tests, commit,
  push, CI refresh, staging, restart и rollback;
- отображать отдельные CI check-runs, commit SHA, время проверки, итог и
  ограниченный текст причины без секретов и необезличенного вывода;
- связать staging/health-marker/rollback с единой evidence-моделью: что было
  проверено, какой commit установлен, почему rollback выполнен или пропущен;
- закрепить, что failure CI не открывает следующий шаг и что каждая опасная
  операция остаётся отдельным явным кликом;
- добавить retention и redaction тесты для repair evidence, чтобы bounded
  projection не превращался в журнал исходного репозитория.

### 21.3 — Crash recovery и диагностика

- определить безопасный startup reconciliation для shell, Core, supervisor,
  transaction worker и незавершённого repair-run;
- показывать пользователю различие между crash, interrupted, unknown outcome,
  rollback и временной недоступностью Core;
- добавить bounded diagnostic bundle: версии компонентов, commit, последние
  типизированные состояния, event sequence и redacted log excerpts;
- запретить попадание provider keys, DPAPI payloads, raw prompts, tool output и
  содержимого workspace в bundle;
- покрыть повторный запуск, устаревшие события, повреждённый projection и
  незавершённую транзакцию deterministic tests.

### 21.4 — Credential, backup/restore и Windows upgrade hardening

- проверить UX хранения provider credentials через DPAPI и рассмотреть
  Credential Manager только как локальную реализацию без изменения внешнего
  контракта;
- сделать в UI понятными preview, safety backup, checksum, progress, cancel,
  rollback и redacted audit для backup/restore;
- провести матрицу upgrade/install/rollback на поддержанных Windows 10 2004+
  и Windows 11, включая locked files, single-instance, Job Object и health
  timeout;
- добавить release evidence с версиями Windows, архитектурой, режимом сборки,
  коммитом и результатом каждого сценария.

## Контракты и изменения

- Core остаётся владельцем причин, переходов, evidence и разрешённых действий.
- IPC-изменения только additive; для новых полей — bounded enums/strings,
  sequence replay, size limits и обновление Rust/Electron/compatibility tests.
- Renderer получает projection и команды, но не получает raw logs, secrets,
  prompts, tool output или workspace contents.
- SQLite-миграции additive и транзакционные; перед изменением схемы создаётся
  backup. Если новая evidence-модель не требует хранения, предпочтителен
  существующий storage и redacted projection.
- Все операции восстановления повторно проверяют policy, capability и
  authenticated session непосредственно перед эффектом.

## Acceptance gates

- unit и contract tests на каждое состояние и каждое разрешённое действие;
- replay/recovery tests для stale, duplicate, out-of-order и corrupted events;
- Electron typecheck, protocol check, UI tests и real-Core E2E для approval,
  repair, rollback и diagnostic bundle;
- Rust Core/storage/IPC/supervisor tests, `cargo fmt --check` и
  `git diff --check`;
- redaction/privacy gate доказывает отсутствие секретов и raw workspace data;
- Windows CI: package startup, single-instance, Job Object, installer,
  upgrade, health timeout, rollback и recovery незавершённой транзакции;
- release evidence содержит команды, дату, commit SHA и bounded результаты,
  а документация обновлена в `architecture.md`, `current-state.md`,
  `decision-register.md` и `release-audit.md` только после фактического
  выполнения этапов.

## Результат после реализации

Ева не пытается «починить» систему сама. Она объясняет, что произошло,
показывает проверяемое evidence и предлагает только те действия, которые Core
разрешил в текущем состоянии. Необратимые или потенциально опасные шаги по-
прежнему требуют отдельного подтверждения пользователя.
