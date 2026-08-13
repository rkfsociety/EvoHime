# Подплан 1 — native hardening, secrets и переносимые проверки

Статус: следующий самый простой подплан
Порядок: 1 из 5
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Закрыть оставшиеся небольшие product-hardening задачи и сделать проверки воспроизводимыми на Windows 11. Backup/restore и crash recovery выполняются как Core-first MVP: UI получает минимальные действия и состояние через IPC, без отдельного полноформатного backup-продукта или нового агентного orchestration loop.

## Объём

- заменить POSIX-зависимые тестовые команды `true`/`false` на кроссплатформенный Rust mock binary `test-stub-exitcode` внутри workspace; зафиксировать, какие тесты его используют, и не объявлять WSL поддерживаемой native-средой;
- завершить хранение provider secrets через Credential Manager/DPAPI с ротацией и удалением старых значений;
- добавить Core-first backup/restore SQLite с JSON preview, фазовым progress, approval и audit; UI ограничить одной командой создания/восстановления файла и отображением IPC progress/error;
- добавить минимальный crash-recovery UI для состояний `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL`, `FAILED`, без отдельного сложного workflow-экрана;
- закрыть security gaps: фильтрация результатов `filesystem.search`, расширенный blocklist интерпретаторов, проверка policy subject и ограничений Git remote;
- выполнить upgrade/install smoke на чистой Windows 11 22H2+ с проверкой rollback при нехватке диска.

## Зафиксированные решения по границам

### Что не входит в этот подплан

- отдельный backup browser, табличный diff, просмотр первых N строк и сложный визуальный progress dashboard;
- автоматическая ротация secret по расписанию;
- запуск Core как Windows Service/`SYSTEM` с отдельным machine-wide secret store;
- полноценный recovery wizard. Эти функции могут быть отдельными последующими этапами после закрытия Core hardening MVP.

### Переносимые проверки

- В workspace добавляется маленький Rust mock binary `test-stub-exitcode`, принимающий exit code аргументом и завершающийся с ним. `std::process::Command` запускает его с явным executable/path и аргументами, без shell parsing; тесты не зависят от `true`, `false`, Bash, `cmd.exe` или WSL.
- Список мест, где использовались POSIX-команды, фиксируется в regression test или комментарии рядом с фикстурой. Linux/macOS остаются валидными средами для Rust-тестов, но эта фикстура не должна ухудшать их совместимость.
- WSL не является reference-средой для WinUI, supervisor и native packaging. При наличии WSL в CI допускается отдельная диагностическая проверка, но её результат не смешивается с Windows acceptance criteria.
- IPC portability tests отдельно проверяют временные каталоги и endpoints: Unix-style `/tmp`/Unix sockets не должны быть скрытой зависимостью; Windows path и named pipe формируются штатными platform APIs.

### Секреты и миграция

- Provider secrets хранятся как Generic Credentials в Windows Credential Manager. Если для локального формата нужен дополнительный encrypted blob, он защищается DPAPI под текущим пользователем; в SQLite/settings хранится только logical credential ID/reference, а не plaintext или самодостаточный API key.
- В production fallback в plaintext settings, логах или SQLite не допускается. Credential Manager/DPAPI привязаны к профилю Windows и машине; восстановление на другой машине выполняется повторной авторизацией или явным импортом секрета пользователем.
- Для dev/CI разрешается только ephemeral fallback через переменную окружения или секрет CI, действующий на время процесса и не попадающий в snapshots, traces, exports и логи. Он не должен автоматически становиться постоянным хранилищем.
- Ротация выполняется по схеме `write new credential -> verify access -> atomically switch reference -> delete old credential`; при ошибке проверки или переключения старое значение и reference сохраняются. Документация должна описывать повторную авторизацию, восстановление после повреждения профиля и удаление отозванного секрета.
- Ротация в scope подплана запускается вручную из UI/CLI; автоматическое расписание не требуется. Если Credential Manager недоступен или нет интерактивной Windows-сессии, production operation завершается безопасной ошибкой и не откатывается к plaintext/мастер-паролю-файлу; CI использует только описанный ephemeral secret.
- Core запускается supervisor'ом от имени вошедшего интерактивного Windows-пользователя. Работа как Windows Service или под `SYSTEM` не входит в поддерживаемый режим этого подплана; при обнаружении другой identity Core завершается fail closed с безопасным diagnostic event и не пытается читать чужой Credential Manager.
- Миграция legacy plaintext settings сначала читает и проверяет значение, записывает Credential Manager и reference, затем атомарно переписывает settings без старого значения, удаляет временные копии и повторно сканирует конфигурацию. При любой ошибке исходный файл сохраняется для повторной миграции, но секрет не выводится в ошибку или лог.

### SQLite backup/restore

- Backup выполняется через SQLite Online Backup API, а не копированием открытого `.db`. В backup входит SQLite database и минимальный несекретный app metadata manifest (schema/app version, timestamps, format version); provider secrets и их значения туда не входят.
- Restore запускается только после Core-level Connection Pool Drain: новые DB operations блокируются, активные транзакции корректно завершаются или отменяются по timeout, соединения закрываются/unmount выполняется до замены файлов, а pool повторно инициализируется только после успешного restore/reopen. Восстановление до инициализации pool допускается как эквивалентный startup path.
- Перед restore автоматически создаётся safety/pre-restore backup текущего состояния тем же безопасным механизмом.
- `preview` — версионируемый JSON-план: источник, время, schema/app version, размер, список затрагиваемых объектов, ожидаемые миграции и потенциальные конфликты; preview не содержит секретов.
- `progress` сообщает фазу (`prepare`, `backup`, `validate`, `restore`, `reopen`, `cleanup`), completed/total для определимых операций и человекочитаемую ошибку. Процент показывается только там, где total достоверен.
- Restore выполняется во временную копию с integrity/schema validation, затем проходит reopen, migrations/reconciliation и только после успешной проверки — атомарную замену. Сбой миграции, валидации, записи, reopen/reconciliation или переполнения диска оставляет рабочую БД нетронутой и предлагает rollback к pre-restore backup; если ошибка произошла после замены, исходное состояние восстанавливается из safety backup. Частично записанная временная копия удаляется после audit.
- Пользовательский preview показывает безопасное summary из JSON-плана: версии, размер, число/типы объектов, ожидаемые миграции и конфликты; первые N строк и diff данных не показываются. Approval — отдельное явное подтверждение перед restore, а не побочный эффект crash recovery.
- Audit записывается в существующий redacted Core JSONL event journal с operation id, actor, timestamps, phase/result и error category; значения секретов и содержимое записей не логируются. Отдельный зашифрованный audit-файл не требуется.
- Regression test подаёт повреждённый/неполный backup и проверяет контролируемую ошибку в UI, отсутствие падения Core, сохранение рабочей БД и audit результата.
- Для Windows locking tests включают WAL mode, активного reader и попытку restore; writer lock/transaction boundary должны корректно завершиться до замены, а незавершённая транзакция не должна попасть в backup.

### Crash-recovery UI

- UI остаётся частью обычного состояния приложения, а не отдельным постоянно блокирующим modal dialog. `RECOVERING` показывает progress и временно отключает конфликтующие команды.
- `BLOCKED` и `WAITING_APPROVAL` являются блокирующими для конкретной операции: UI явно показывает причину, затронутый ресурс, требуемое действие и кнопки `reconcile`/`approve`/`cancel` только когда они разрешены Core. Неизвестный effect нельзя подтвердить вслепую.
- `FAILED` показывает безопасное описание, request/correlation id и доступные действия (`retry`, `restore`, `open diagnostics`); повтор не должен выполняться автоматически без политики retry.

| State | UI показывает | Допустимые действия |
| --- | --- | --- |
| `RECOVERING` | recovery/reconciliation progress и текущую фазу | `wait`; `cancel` только если Core подтвердил безопасную отмену |
| `WAITING_APPROVAL` | неизвестный или неподтверждённый effect, ресурс и последствия | `approve`, `reject` |
| `BLOCKED` | причину блокировки и условие разблокировки | `retry`, `resolve`, `open details` |
| `FAILED` | последнюю безопасно отредактированную ошибку и correlation id | `retry`, `abort`, `open details` |

Эта матрица является контрактом UI/Core: неизвестный effect может перейти к `approve` только после reconciliation и явного approval, а недоступное действие не отображается. UI получает state transitions и progress через versioned IPC event stream с request id/sequence replay, а не через прямой доступ к Core/SQLite или обязательный polling.

### Search и security edge cases

- `filesystem.search` фильтрует результаты по canonical path после разрешения symlink/reparse-point, проверяет containment относительно разрешённых roots и повторно применяет ту же path/policy validation, что и прямой filesystem access, перед выдачей. Проверка относится к пути и содержимому результата, а не к любому совпадению имени легитимного файла.
- Regression tests покрывают symlink/reparse escape, `..`, alternate path forms, Unicode normalization и похожие символы. Unicode-омографы проверяются как обход сопоставления запрещённых имён/команд, а не объявляются запрещёнными сами по себе.
- Blocklist интерпретаторов запрещает прямые `cmd`/`powershell`/`python` и indirect launcher aliases или переименованные executable paths, если это предусмотрено policy model; позитивные тесты подтверждают, что разрешённые инструменты не блокируются.
- Запрет относится к запуску interpreter/launcher процессов и их обходам, а не к текстовым словам `eval`/`exec` в обычных пользовательских данных; policy явно перечисляет executable families и indirect launchers.
- Policy subject проверяется после canonical resolution против canonical resolved subject, а не против пользовательского display name или неподтверждённой строки input.
- По умолчанию `filesystem.search` исключает `.env*`, `.git/`, `*.pem`, `*.key`, `secrets.yml` и эквивалентные canonical/normalized forms. Конфигурация может добавлять ограничения, но не ослабляет hard defaults без отдельной policy/approval.
- Hard defaults также исключают `.svn/`, `id_rsa`/`id_ed25519`, системные credential/token stores и распространённые auth-файлы; конфигурация может только добавлять ограничения и не ослабляет defaults без отдельной policy/approval.
- Git remote разрешает только ожидаемые `https`, `ssh` и `git` forms с allowlisted host и repository scope; tests покрывают `file://`, UNC, arbitrary SSH hosts, credential-bearing URLs, смену origin и redirect/canonicalization cases. Запрещённые remote не должны обходить policy через alias, redirect или альтернативную форму URL.

### Windows reference и rollback

- Гейт — чистая x64 Windows 11 22H2 с последними доступными cumulative updates на момент прогона; версия, build, архитектура, свободное место и результат фиксируются в артефакте smoke-теста.
- Insider Preview не является обязательным гейтом этого подплана, но может использоваться как informative compatibility run с отдельным результатом.
- Upgrade/install проверяет нехватку диска и сбой на каждом этапе, сохранение текущей рабочей версии, очистку staging и повторный запуск после освобождения места. Rollback должен быть идемпотентным и не удалять пользовательские данные.

Smoke-матрица на reference-системе: clean install → launch → configure provider secret → создать проверяемое DB state → upgrade `N` → `N+1` → симулировать failed migration/startup → rollback → проверить DB и доступность secret reference → uninstall/reinstall согласно зафиксированной policy сохранения пользовательских данных. Для каждого сценария записываются версия/build, exit result, логи без секретов и итоговое состояние данных. Автоматизированный GitHub Actions `windows-latest` smoke выполняет доступную install/upgrade/recovery часть как regression gate; чистая 22H2 VM остаётся release acceptance reference.

## Порядок реализации

1. Исправить переносимость тестовых фикстур и прогнать Rust/WinUI/IPC проверки.
2. Вынести секреты из обычных настроек в Credential Manager/DPAPI; добавить тесты отсутствия секретов в logs/traces/exports.
3. Реализовать Core backup/restore и минимальные UI-команды/состояния поверх существующего Core recovery state; не добавлять отдельный backup browser или recovery wizard.
4. Закрыть search/interpreter/policy edge cases отдельными regression tests, включая symlink/reparse-point, Unicode и Git remote audit cases.
5. Проверить установку, обновление, rollback и recovery на чистой Windows 11 reference-системе; отдельно записать informative результат Insider/WSL, если такой прогон доступен.

## Критерии готовности

- `cargo test --workspace`, WinUI tests и IPC tests проходят без environment-only failures;
- POSIX-команды не требуются для native-тестов, а WSL не используется как скрытая зависимость acceptance run;
- provider key и распространённые encoded forms не хранятся в settings, logs, traces, prompts/tool payload history, audit events, diagnostic bundle/Core Doctor export, backup metadata или crash dump annotations;
- synthetic crash-dump/core-dump test не содержит provider key или encoded forms в проверяемых annotations и serialized diagnostic fields; полноценное доказательство отсутствия значения во всей памяти ОС не заявляется этим подпланом;
- fallback для dev/CI является только ephemeral и не сохраняется после завершения процесса;
- preview backup имеет проверяемый JSON-контракт, progress содержит фазу и достоверный прогресс, а ошибка restore не повреждает рабочую БД;
- backup восстанавливается после ошибки миграции и явно показывает область восстановления, pre-restore backup и доступный rollback;
- UI не предлагает продолжить неизвестный effect без reconciliation/approval и явно различает `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL` и `FAILED`;
- `.env`, запрещённые результаты поиска, symlink/reparse escapes и обходы blocklist не выдаются через `filesystem.search`;
- Git remote policy отклоняет запрещённые scheme/host/scope, credential-bearing URLs, UNC/file URLs и redirect/canonicalization обходы;
- Попытки взаимодействия с недоверенными или внешними Git remote блокируются policy check и получают redacted audit event с operation id, причиной и canonical remote scope;
- smoke-матрица покрывает install, launch, secret setup, DB state, upgrade, failed migration/startup, rollback и uninstall/reinstall policy;
- изменения secret reference/API сохраняют versioned IPC compatibility с task runner; Core остаётся единственным владельцем получения секрета, а UI и task runner не получают новый plaintext boundary;
- установщик, rollback и повторное восстановление после нехватки диска проходят на чистой reference Windows 11 22H2+.

## Зависимости

Использует завершённые этапы 0b/0c и Core Doctor. Не блокирует разработку task runner, но должен быть закрыт до release hardening.
