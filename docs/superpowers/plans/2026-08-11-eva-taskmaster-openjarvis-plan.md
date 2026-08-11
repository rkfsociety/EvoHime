# План развития Евы по мотивам Task Master и OpenJarvis

Дата: 2026-08-11
Статус: предложение для следующей native-фазы EvoHime
Область: Rust Core, SQLite, versioned named-pipe IPC, WinUI 3, supervisor

## 1. Что изучено

Изучены актуальные материалы:

- [claude-task-master](https://github.com/eyaltoledano/claude-task-master) и его [документация](https://tryhamster.com/docs/taskmaster): PRD-парсинг, структура задач и подзадач, статусы, зависимости, теги/workstreams, выбор следующей готовой задачи, complexity analysis, research с цитатами и автоматический loop с сохранением прогресса.
- [OpenJarvis](https://github.com/open-jarvis/OpenJarvis), [руководство по навыкам](https://open-jarvis.github.io/OpenJarvis/user-guide/skills/), [deep research](https://open-jarvis.github.io/OpenJarvis/user-guide/deep-research/), [scheduled monitor](https://open-jarvis.github.io/OpenJarvis/user-guide/scheduled-monitor/) и [roadmap](https://open-jarvis.github.io/OpenJarvis/development/roadmap/): local-first execution, каталог skills, on-demand/scheduled/continuous агенты, многошаговое исследование с источниками, память с retrieval/compression, каналы/коннекторы и eval/trace-метрики.

Это не план импорта этих проектов. Идеи должны быть реализованы нативно внутри действующей границы EvoHime и пройти отдельный лицензионный review до использования стороннего кода.

## 2. Цель для EvoHime

Сделать Еву не только чат-клиентом с инструментами, а локальным диспетчером долгих задач разработки и личных операций:

1. пользователь формулирует цель или импортирует PRD;
2. Core строит проверяемый граф задач с зависимостями и критериями готовности;
3. Ева выбирает только разблокированную задачу и работает короткими возобновляемыми итерациями;
4. research, skills, память и модельный fallback подключаются по необходимости;
5. каждый шаг виден в UI, ограничен permissions/budget и оставляет trace;
6. после перезапуска Core работа продолжается с последнего подтверждённого состояния.

## 3. Что уже есть и что является ограничением

У EvoHime уже присутствуют native Core, SQLite, replay-события IPC, model-gateway с провайдерами и fallback-направлением, tool-runtime с filesystem/Git/shell/browser/MCP/memory, permissions, plugin catalog, scheduled-раздел UI, trace и approval-события. Поэтому новые функции нужно достраивать в Core и протоколе, а не дублировать бизнес-логику в WinUI.

Обязательные инварианты:

- UI только отображает reducer-состояние и отправляет IPC-команды.
- Все операции workspace, Git, shell, web, память и расписания выполняются Core.
- Каждый новый IPC-контракт получает compatibility tests, sequence replay и bounded frame size.
- Автоматический режим не получает неограниченное право на shell, сеть, Git commit/push или секреты.
- Секреты остаются в Credential Manager/DPAPI; trace и research проходят redaction.
- Ветка остаётся текущей main; для каждого завершённого этапа — task-only commit, push только по прямому запросу.

## 4. Приоритетный план реализации

### Этап 0. Контракты и данные (P0)

**Результат:** устойчивое доменное ядро задач, на которое опираются остальные функции.

- Добавить в evohime-local-storage миграции для projects, work_items, work_item_edges, work_item_events, work_item_tags, work_item_research, run_checkpoints.
- Определить состояния: backlog, ready, in_progress, blocked, waiting_approval, done, cancelled, failed.
- Хранить родителя, подзадачи, priority, estimate, acceptance criteria, source, tag/workstream, attempt count и последний error.
- Реализовать транзакционную проверку отсутствующих ссылок и циклов; next_ready должен учитывать зависимости и priority.
- Расширить proto командами: task.create/update/list/get/next, dependency.add/remove/validate, task.checkpoint, task.resume и событиями graph/status/progress.
- Написать Rust-тесты миграций, идемпотентности, циклов, гонок двух runners, replay после рестарта и атомарности переходов статуса.

### Этап 1. Task workspace в native UI (P0)

**Результат:** раздел «Проекты/Задачи» показывает реальный граф и не обещает недоступных действий.

- Добавить в Core импорт PRD/Markdown и безопасный режим ручного создания задачи.
- Парсер должен сохранять исходный текст, версию импорта и происхождение каждой задачи; ошибки показывать как actionable diagnostics.
- В WinUI сделать список ready/blocked/done, граф зависимостей, карточку задачи, подзадачи, acceptance criteria и историю событий.
- Добавить действия «Следующая задача», «Разблокировать», «Отложить», «Запустить», «Остановить», «Повторить», «Отметить готовой»; переходы подтверждаются Core.
- Не включать в первую версию автоматическое изменение файлов по одному только импорту PRD.

### Этап 2. Research с цитатами и контекстом проекта (P0)

**Результат:** Ева умеет перед началом работы получать свежий проверяемый контекст и прикреплять его к задаче.

- Добавить Core workflow research: запрос → поиск/HTTP-инструмент → извлечение источников → краткий ответ → citations → сохранение в work_item_research.
- Разделять источники из web, локальных документов и workspace; сохранять URL, title, fetched-at, hash и redacted excerpt.
- Реализовать команды research.start, research.cancel, research.attach, research.refresh; UI показывает источники и устаревшие результаты.
- Добавить лимиты времени, размера ответа, доменов и стоимости; цитаты не должны превращаться в непроверенный prompt-контент.
- Для security/dependency/API-вопросов research должен быть обязательной опцией перед запуском, но решение о запуске остаётся за политикой и пользователем.

### Этап 3. Skills и безопасный каталог расширений (P1)

**Результат:** навыки становятся версионируемыми инструкциями и capabilities, а не произвольным кодом из каталога.

- Расширить существующий plugin catalog манифестом skill: name, version, description, triggers, required tools, network domains, permissions, input/output schema, checksum и source.
- Сделать локальный registry с install/enable/disable/update/rollback и проверкой подписи/хэша до активации.
- Разделить skill-инструкции, MCP-сервера и исполняемые расширения; каждое capability попадает в permission/approval policy.
- Добавить skill.discover и skill.resolve в Core, а в UI — просмотр effective permissions и причин, почему навык не выбран.
- Импорт внешних skills разрешать только после prompt-injection scan, статической проверки манифеста и изолированного smoke run.

### Этап 4. Ограниченный task loop и supervisor orchestration (P1)

**Результат:** Ева может продолжать серию небольших задач, оставаясь возобновляемой и наблюдаемой.

- Реализовать runner: выбрать next_ready → сформировать prompt из задачи/research/skills → запустить один bounded run → записать checkpoint → предложить следующий шаг.
- Ввести run_policy: max iterations, wall-clock timeout, tool-call budget, token budget, network policy, approval mode и stop conditions.
- Автоматически останавливать loop на approval, failure, изменении workspace вне ожидаемого diff, нарушении budget или неоднозначном критерии готовности.
- В supervisor добавить жизненный цикл долгих run, отмену, restart/recovery и очистку дочерних процессов через Job Object.
- Добавить UI для Start/Pause/Resume/Stop, текущего шага, checkpoint, бюджета, причин остановки и dry-run.
- Автоматический commit не делать по умолчанию; commit/push должны оставаться отдельными явно разрешёнными действиями.

### Этап 5. Local-first routing и operational profiles (P1)

**Результат:** выбор модели зависит не только от качества, но и от приватности, задержки и стоимости.

- Расширить model-gateway профилями local-first, balanced, cloud-research, offline.
- Для каждого запроса учитывать capability, context size, tool use, privacy class, latency/price budget и доступность провайдера.
- Поддержать локальный OpenAI-compatible/Ollama endpoint как первый маршрут; cloud fallback только по policy и видимому событию.
- Писать redacted model trace с latency, tokens, retry, provider, estimated cost и результатом маршрутизации; секреты и полный prompt не экспортировать без явного действия.
- В Settings показать effective route и причину выбора, а не только строку с названием модели.

### Этап 6. Память, traces и улучшение качества (P1)

**Результат:** Ева не забывает принятые решения и может измерять, какие навыки/маршруты реально помогают.

- Разделить memory на profile/preferences, project facts, decisions, task history и ephemeral run context с TTL.
- Реализовать retrieval по workspace/project/task, compression старых trace и ссылки на первичный event вместо дублирования больших payloads.
- Добавить пользовательские действия feedback: полезно/неполезно, исправление ответа, причина отказа, успешный/неуспешный tool result.
- Ввести локальный eval-набор для типовых задач EvoHime: чтение/патч/тесты, approval, cancellation, replay, research citations, skill selection и offline route.
- Показывать агрегаты latency, success rate, retries, tool failures и estimated cost; не отправлять телеметрию наружу без opt-in.

### Этап 7. Scheduled monitors и proactive Pulse (P2)

**Результат:** расписания становятся stateful-мониторами, а не одноразовым запуском prompt.

- Реализовать сущности schedule, trigger, monitor state, last checkpoint, next run, retry/backoff и dead-letter reason.
- Добавить безопасные preset-ы: проверка GitHub notifications, изменения workspace, статуса CI, локальных файлов и истечения сроков задач.
- Запускать monitor через supervisor с теми же budgets, permissions, approvals и cancellation, что и обычный task loop.
- В Pulse показывать digest, новые события, пропущенные запуски и деградацию; уведомления не должны скрывать failure.
- Для первой версии не подключать внешние календари/почту: сначала отладить локальные источники и протокол, затем добавлять коннекторы по отдельным RFC.

## 5. Рекомендуемый порядок поставки

1. Этап 0 — доменная модель и IPC; без него остальные функции будут временными обходами.
2. Этап 1 — task workspace и ручной workflow; это даст проверяемый UI-контур.
3. Этап 2 — research с citations; затем Этап 3 — skills и permissions.
4. Этап 4 — bounded loop только после готовых checkpoint/approval/replay.
5. Этапы 5–6 — routing, память и evals для измеримого качества.
6. Этап 7 — scheduled monitors после стабилизации lifecycle supervisor.

## 6. Что не следует добавлять сейчас

- Не переносить Python runtime OpenJarvis, Node/MCP Task Master или их CLI внутрь продукта.
- Не возвращать веб-панель, Vite или прямой UI-доступ к SQLite/workspace.
- Не разрешать автономный shell/Git push, бесконечные loops и произвольные внешние skills.
- Не делать cloud-first routing и не сохранять API keys в конфигурации репозитория.
- Не копировать код, UI, названия, ассеты или точную структуру внешних продуктов; брать только проверяемые идеи и отдельно проверить лицензии.

## 7. Критерии готовности всей инициативы

- Core переживает restart во время research, tool call и loop и корректно воспроизводит состояние через IPC replay.
- Для каждой задачи видны зависимости, источник, критерии готовности, trace, budget и причина текущего статуса.
- Любое автоматическое действие имеет policy, timeout, cancellation, approval и redacted diagnostic trail.
- Offline/local route работает без облачного провайдера; fallback не происходит молча.
- Skills и monitors проходят manifest/permission checks и могут быть отключены/откачены.
- Rust tests, C# IPC/UI tests, native workflow smoke, git diff --check и package smoke проходят на Windows.
- После проверок удаляются target, bin, obj и временные package artifacts; изменения фиксируются task-only коммитами.
