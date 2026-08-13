# Подплан 1 — native hardening, secrets и переносимые проверки

Статус: следующий самый простой подплан
Порядок: 1 из 5
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Закрыть оставшиеся небольшие product-hardening задачи и сделать проверки воспроизводимыми на Windows 11. Этот подплан не добавляет новый агентный orchestration loop.

## Объём

- заменить POSIX-зависимые тестовые команды `true`/`false` на Windows-совместимую тестовую фикстуру; зафиксировать, какие тесты её используют, и не объявлять WSL поддерживаемой native-средой;
- завершить хранение provider secrets через Credential Manager/DPAPI с ротацией и удалением старых значений;
- добавить пользовательский backup/restore SQLite с JSON preview, фазовым progress, approval и audit;
- добавить crash-recovery UI для состояний `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL`, `FAILED`;
- закрыть security gaps: фильтрация результатов `filesystem.search`, расширенный blocklist интерпретаторов, проверка policy subject и ограничений Git remote;
- выполнить upgrade/install smoke на чистой Windows 11 22H2+ с проверкой rollback при нехватке диска.

## Зафиксированные решения по границам

### Переносимые проверки

- Тестовая фикстура должна запускать короткий процесс, возвращающий заданный exit code, через API/исполняемый файл, доступный в Windows CI; тесты не должны зависеть от наличия `true`, `false`, Bash или WSL.
- Список мест, где использовались POSIX-команды, фиксируется в regression test или комментарии рядом с фикстурой. Linux/macOS остаются валидными средами для Rust-тестов, но эта фикстура не должна ухудшать их совместимость.
- WSL не является reference-средой для WinUI, supervisor и native packaging. При наличии WSL в CI допускается отдельная диагностическая проверка, но её результат не смешивается с Windows acceptance criteria.

### Секреты и миграция

- В production fallback в plaintext settings, логах или SQLite не допускается. Credential Manager/DPAPI привязаны к профилю Windows и машине; восстановление на другой машине выполняется повторной авторизацией или явным импортом секрета пользователем.
- Для dev/CI разрешается только ephemeral fallback через переменную окружения или секрет CI, действующий на время процесса и не попадающий в snapshots, traces, exports и логи. Он не должен автоматически становиться постоянным хранилищем.
- Ротация выполняется по схеме `write new -> verify access -> delete old`; при ошибке проверки старое значение сохраняется. Документация должна описывать повторную авторизацию, восстановление после повреждения профиля и удаление отозванного секрета.

### SQLite backup/restore

- `preview` — версионируемый JSON-план: источник, время, schema/app version, размер, список затрагиваемых объектов, ожидаемые миграции и потенциальные конфликты; preview не содержит секретов.
- `progress` сообщает фазу (`prepare`, `backup`, `validate`, `restore`, `reopen`, `cleanup`), completed/total для определимых операций и человекочитаемую ошибку. Процент показывается только там, где total достоверен.
- Restore выполняется во временную копию с integrity/schema validation и атомарной заменой. До замены создаётся pre-restore backup. Сбой миграции, валидации, записи или переполнения диска оставляет рабочую БД нетронутой и предлагает rollback к pre-restore backup; частично записанная временная копия удаляется после audit.

### Crash-recovery UI

- UI остаётся частью обычного состояния приложения, а не отдельным постоянно блокирующим modal dialog. `RECOVERING` показывает progress и временно отключает конфликтующие команды.
- `BLOCKED` и `WAITING_APPROVAL` являются блокирующими для конкретной операции: UI явно показывает причину, затронутый ресурс, требуемое действие и кнопки `reconcile`/`approve`/`cancel` только когда они разрешены Core. Неизвестный effect нельзя подтвердить вслепую.
- `FAILED` показывает безопасное описание, request/correlation id и доступные действия (`retry`, `restore`, `open diagnostics`); повтор не должен выполняться автоматически без политики retry.

### Search и security edge cases

- `filesystem.search` фильтрует результаты по canonical path после разрешения symlink/reparse-point, проверяет containment относительно разрешённых roots и повторно применяет redaction/blocklist перед выдачей. Проверка относится к пути и содержимому результата, а не к любому совпадению имени легитимного файла.
- Regression tests покрывают symlink/reparse escape, `..`, alternate path forms, Unicode normalization и похожие символы. Unicode-омографы проверяются как обход сопоставления запрещённых имён/команд, а не объявляются запрещёнными сами по себе.
- Расширенный blocklist интерпретаторов, policy subject и Git remote restrictions должны иметь позитивные и негативные тесты: разрешённые имена не блокируются, а обход через alias/path/регистронезависимое и Unicode-представление не проходит.

### Windows reference и rollback

- Гейт — чистая x64 Windows 11 22H2 с последними доступными cumulative updates на момент прогона; версия, build, архитектура, свободное место и результат фиксируются в артефакте smoke-теста.
- Insider Preview не является обязательным гейтом этого подплана, но может использоваться как informative compatibility run с отдельным результатом.
- Upgrade/install проверяет нехватку диска и сбой на каждом этапе, сохранение текущей рабочей версии, очистку staging и повторный запуск после освобождения места. Rollback должен быть идемпотентным и не удалять пользовательские данные.

## Порядок реализации

1. Исправить переносимость тестовых фикстур и прогнать Rust/WinUI/IPC проверки.
2. Вынести секреты из обычных настроек в Credential Manager/DPAPI; добавить тесты отсутствия секретов в logs/traces/exports.
3. Реализовать backup/restore и crash-recovery UI поверх существующего Core recovery state.
4. Закрыть search/interpreter/policy edge cases отдельными regression tests, включая symlink/reparse-point и Unicode cases.
5. Проверить установку, обновление, rollback и recovery на чистой Windows 11 reference-системе; отдельно записать informative результат Insider/WSL, если такой прогон доступен.

## Критерии готовности

- `cargo test --workspace`, WinUI tests и IPC tests проходят без environment-only failures;
- POSIX-команды не требуются для native-тестов, а WSL не используется как скрытая зависимость acceptance run;
- provider key не хранится в plaintext settings, prompt, trace или JSONL;
- fallback для dev/CI является только ephemeral и не сохраняется после завершения процесса;
- preview backup имеет проверяемый JSON-контракт, progress содержит фазу и достоверный прогресс, а ошибка restore не повреждает рабочую БД;
- backup восстанавливается после ошибки миграции и явно показывает область восстановления, pre-restore backup и доступный rollback;
- UI не предлагает продолжить неизвестный effect без reconciliation/approval и явно различает `RECOVERING`, `BLOCKED`, `WAITING_APPROVAL` и `FAILED`;
- `.env`, запрещённые результаты поиска, symlink/reparse escapes и обходы blocklist не выдаются через `filesystem.search`;
- установщик, rollback и повторное восстановление после нехватки диска проходят на чистой reference Windows 11 22H2+.

## Зависимости

Использует завершённые этапы 0b/0c и Core Doctor. Не блокирует разработку task runner, но должен быть закрыт до release hardening.
