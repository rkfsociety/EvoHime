# 19.0 — Пользовательское самоисправление и самообновление

## Статус

Черновой обзорный план. Реализацию запускать только после отдельного ревью
этого контракта. План не включает автоматический запуск по таймеру, по факту
ошибки или при старте установленной Евы.

## Цель

Добавить в установленную EvoHime безопасный пользовательский цикл ремонта
собственного репозитория и последующего обновления клиента:

```text
накопились ошибки
    ↓
пользователь нажал «Починить»
    ↓
Ева собирает bounded-диагностику и предлагает исправление
    ↓
изолированная копия → patch → тесты → diff
    ↓
пользователь подтверждает применение/commit
    ↓
пользователь отдельно подтверждает push
    ↓
GitHub Actions проверяет commit
    ↓
зелёный installer готов
    ↓
пользователь отдельно подтверждает перезапуск и обновление
```

После успешного обновления Ева должна стартовать на новой версии или
безопасно вернуться к предыдущей. Ни один шаг не должен молча менять `main`,
публиковать код или перезапускать установленный клиент.

## Что уже есть и переиспользуется

- Core уже классифицирует ошибки инструментов, добавляет recovery-подсказки,
  ограничивает повторения и сохраняет project-scoped failure lessons.
- В Core уже есть `filesystem.patch`/`filesystem.write`, тестовые команды,
  `git.status`, `git.diff`, `git.commit`, `git.pull` и `git.push` с policy и
  approval.
- Electron updater уже проверяет зелёные GitHub commits, ждёт опубликованный
  installer, показывает предложение перезапуска и использует transaction
  worker с backup/rollback.
- Установленный клиент уже знает собственный repository URL, branch, commit,
  staging directory и update journal.

Источники текущего поведения: `crates/evohime-core/src/lib.rs`,
`crates/tool-runtime/src/tools/git.rs`,
`desktop/evohime-electron/src/main/update/update-service.ts`,
`docs/architecture.md` и `docs/current-state.md`.

## Не входит в план

- запуск ремонта без клика пользователя;
- автоматический push после успешных тестов;
- автоматическое изменение GitHub Actions, policy, permission, updater,
  supervisor, receipt-кода или секретной инфраструктуры;
- принятие сгенерированного patch без просмотра diff и результатов тестов;
- push force, `--no-verify`, прямое удаление веток или публикация в чужой
  репозиторий;
- обучение весов ChatGPT/Codex или передача им токенов ChatGPT/GitHub;
- установка локально собранного непроверенного production-пакета.

## Пользовательский интерфейс

### Индикатор накопленных ошибок

В Operations/diagnostics projection показывать:

- число ошибок за bounded окно;
- число повторяющихся failure patterns;
- затронутый workspace и последнюю безопасную причину;
- дату последнего repair-run;
- кнопку `Починить` только если есть actionable ошибка.

Ошибки сами по себе только создают предложение. Они не запускают repair-run.
Кнопка должна быть недоступна при отсутствии выбранного workspace, активном
repair-run или неизвестном состоянии Core.

### Экран repair-run

Отдельные состояния и действия:

1. `Предложен` — показать сводку ошибок и изменяемый репозиторий.
2. `Диагностика` — собрать факты, не менять файлы.
3. `Исправление подготовлено` — показать bounded diff и список затронутых
   файлов.
4. `Тестирование` — показать команды, exit code, duration и краткий результат.
5. `Готово к commit` — отдельная кнопка `Применить и закоммитить`.
6. `Готово к push` — отдельная кнопка `Отправить в GitHub`.
7. `Ожидание CI` — показывать check-runs/workflow conclusion, но не считать
   отсутствие ответа успехом.
8. `Готово к обновлению` — отдельная кнопка `Обновить Еву`.
9. `Завершено` или `Откат выполнен`.

Для любого отказа показывать recoverable state и причину. Сырой вывод модели,
секреты, токены и необезличенный вывод внешних инструментов в renderer не
передавать.

## Архитектурный контракт

### Владелец состояния

Core владеет repair-run, его FSM, попытками, approvals, evidence refs,
commit SHA и terminal outcome. Electron только показывает projection и
отправляет команды. SQLite хранит metadata и redacted evidence; большие
логи/patch сохраняются через существующий bounded artifact/provenance путь.

### Изолированная копия

Repair не работает в открытом пользовательском workspace и не редактирует
каталог установленного приложения. На старте создаётся отдельная source
checkout под `%LOCALAPPDATA%\EvoHime\repair\<repair-id>` из закреплённого
repository URL и базового commit. Репозиторий должен совпадать с
конфигурацией продукта; произвольный URL из prompt запрещён.

Рабочая ветка имеет bounded имя вида `evohime/repair/<repair-id>`. Ветка
удаляется только после terminal outcome и явного retention policy; исходный
`main` не переключается.

### Модель ремонта

Ева передаёт ChatGPT/Codex только:

- классифицированные ошибки;
- минимальные redacted tool findings;
- bounded relevant source/test context;
- правила проекта и acceptance criteria.

Модель сначала обязана предложить причину и план проверки, затем применить
patch только через Core tools. Сгенерированный текст не является доказательством
успеха: acceptance определяется тестами, diff и отдельными policy gates.

### Изменяемые файлы

По умолчанию разрешены файлы продукта и тестов. Изменения в следующих областях
либо блокируются, либо получают отдельный высокий-risk approval:

- `AGENTS.md`, `.codex`, permission/loadout и security policy;
- updater, transaction worker, supervisor и receipt/audit код;
- GitHub Actions и release scripts;
- `.env*`, ключи, credentials, signing material и generated secrets.

Если repair затрагивает release/security-контур, обычная кнопка ремонта должна
остановиться со статусом `manual_review_required`, а не продолжать до push.

## Git и GitHub Actions

1. До ремонта Core фиксирует базовый commit и чистое состояние checkout.
2. После patch обязательно выполняются targeted tests, затем требуемые
   project gates и `git diff --check`.
3. Commit создаётся только отдельной кнопкой с task-only сообщением.
4. Push выполняется только отдельной кнопкой, с повторным показом remote,
   branch и commit SHA. Force push и небезопасные flags запрещены.
5. Предпочтительный режим — push repair-ветки и проверка CI до продвижения в
   `main`. Публикация в `main` требует второго явного подтверждения.
6. GitHub API проверяется по commit/check-runs. `queued`, `in_progress`, API
   error, missing checks и unknown conclusion не являются зелёным результатом.
7. Для CI failure Ева показывает ссылки/коды проверок и предлагает новый
   repair iteration, но не исправляет и не пушит следующий вариант сама.

## Самообновление

Repair-run не подменяет установленный `EvoHime.exe` напрямую. После зелёного
commit updater использует существующую production-схему:

- ждёт опубликованный CI installer для того же SHA;
- сверяет manifest, SHA-256 и target branch;
- скачивает в staging;
- показывает пользователю кнопку `Обновить Еву`;
- transaction worker делает backup и атомарную замену;
- при ошибке подмены восстанавливает предыдущую установку;
- незавершённая транзакция восстанавливается при следующем запуске.

После запуска новой версии нужен bounded health handshake: Electron, supervisor
и Core должны подтвердить IPC/authenticated startup. При повторяющемся startup
failure transaction worker/launcher возвращает предыдущий пакет и сохраняет
redacted recovery evidence.

## Безопасность и отказоустойчивость

- Один repair-run на repository/workspace; повторная кнопка идемпотентно
  возвращает текущий run.
- Все transitions и approval IDs durable; crash не превращает неизвестный
  push/install outcome в успешный.
- Лимиты: размер checkout, число файлов, размер diff, число итераций ремонта,
  длительность тестов, число CI polling запросов и срок хранения артефактов.
- GitHub token используется только для GitHub API/credential helper, не входит
  в prompt, logs, SQLite evidence или command arguments.
- Network/API unavailable всегда даёт `degraded`/`blocked`, а не обход проверки.
- Установленная версия продолжает запускаться при любой ошибке repair/update;
  кнопка `Пропустить` сохраняется на launch gate.
- Каждое значимое действие получает audit event: предложен run, patch,
  approval, test result, commit, push, CI conclusion, installer SHA, restart,
  rollback.

## Этапы реализации

### 19.1 — Error digest и пользовательский запуск

Добавить Core-owned bounded error digest, IPC projection и UI-кнопку
`Починить`. Реализовать FSM repair-run без изменений исходников: proposal,
diagnostics, cancel, retention и recovery после перезапуска.

**Блокирующие зависимости:** текущие task/tool metrics, authenticated IPC,
OperationsPanel, durable SQLite migrations.

**Опциональные зависимости:** расширенная GitHub API диагностика; без неё
показывается только локальное состояние ошибок.

### 19.2 — Изолированная диагностика и patch

Добавить безопасную source checkout/repair workspace, baseline checks,
diagnostic prompt для ChatGPT/Codex, protected-path policy и bounded patch
projection. До approval никакие изменения не попадают в основной checkout.

**Блокирующие зависимости:** 19.1, текущие filesystem/git tools, source
update path.

**Опциональные зависимости:** Codex CLI app-server; fallback — configured
OpenAI Responses provider с тем же Core tool gate.

### 19.3 — Validation, commit и push gates

Добавить обязательный test plan, запуск project gates, diff review, отдельные
commit/push approvals, repair-branch lifecycle и GitHub check-runs polling.
Проверить, что после CI failure repair-run остаётся возобновляемым, но не
продолжает сам.

**Блокирующие зависимости:** 19.2, git policy/approval, GitHub repository
configuration.

**Опциональные зависимости:** PR API; без неё продвижение выполняется только
явным push в заранее разрешённую ветку.

### 19.4 — Installer handoff, health gate и rollback

Связать зелёный repair commit с существующим installer updater, добавить
кнопку `Обновить Еву`, post-restart health handshake, rollback evidence и
acceptance matrix для failed build, failed CI, missing installer, locked files,
Core crash и повторного запуска после незавершённой транзакции.

**Блокирующие зависимости:** 19.3, current transaction worker, package smoke,
authenticated Core startup tests.

**Опциональные зависимости:** локальная `launchPolicy=build` для dev-only
тестов; production остаётся installer-only.

## Критерии готовности направления

- Без клика пользователя repair-run никогда не стартует.
- Без отдельного подтверждения нет patch, commit, push или restart.
- Невозможно незаметно изменить `main`, policy, updater или secrets.
- Один и тот же repair-run переживает перезапуск и не повторяет неизвестный
  внешний эффект вслепую.
- CI failure, API outage и невалидный installer видны как отказ/degraded.
- Успешный run доказывается commit SHA, test evidence, green check-runs,
  installer manifest/SHA-256 и authenticated startup новой версии.
- Неуспешный install возвращает предыдущую рабочую версию.
- Есть focused Rust/Electron tests, real-Core E2E, source-update E2E,
  package/rollback smoke и `git diff --check`.
