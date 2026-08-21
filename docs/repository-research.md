# Анализ внешних репозиториев для EvoHime

Рабочий журнал исследования репозиториев, которые могут дать Еве полезные
идеи, код, контракты, тесты или инженерные практики. Записи не являются
утверждённым состоянием продукта и не означают автоматического принятия
зависимости или копирования кода.

## Как оцениваем

Для каждого репозитория проверяем:

- назначение и ключевые возможности;
- архитектуру и совместимость с Electron + Rust Core + supervisor;
- лицензию, происхождение и ограничения повторного использования;
- качество кода, тестов, документации и активность проекта;
- безопасность, приватность, sandbox, сеть, хранение секретов и модель угроз;
- что можно перенять как идею, контракт, тестовый подход или код;
- стоимость адаптации, риски и способ изоляции интеграции;
- связь с текущими планами и отсутствие конфликта с уже реализованными
  границами EvoHime.

## Реестр

| № | Репозиторий | Статус | Потенциальная ценность | Решение |
|---|---|---|---|---|

## Карточки исследований

Для каждого источника добавляется отдельная карточка:

```markdown
### N. Название репозитория

- Источник:
- Дата проверки:
- Ревизия/commit:
- Лицензия:
- Назначение:
- Краткий вывод:

#### Что изучено

- архитектура и основные модули;
- точки интеграции;
- тесты и проверяемые гарантии;
- документация и история изменений.

#### Что можем использовать в Еве

- идея/паттерн:
- контракт или формат:
- код или библиотека:
- тесты/fixtures:

#### Ограничения и риски

- лицензия и attribution:
- безопасность и приватность:
- несовместимость с архитектурой EvoHime:
- стоимость сопровождения:

#### Предварительное решение

`использовать` / `адаптировать` / `наблюдать` / `отклонить`

#### Связь с EvoHime

- затронутые документы или планы:
- предполагаемый этап интеграции:
- критерии проверки:
```

### 1. Superagent SDK

- Источник: https://github.com/superagent-ai/superagent
- Дата проверки: 2026-08-21
- Ревизия/commit: `aa6c184972fb6fe29d3bf41f12c8f46d7c4262d8`
- Лицензия исходного кода: MIT
- Состав: TypeScript SDK, Python SDK, CLI и stdio MCP-сервер;
  отдельная web-документация и OpenAPI.
- Назначение: внешние LLM-guardrails для классификации prompt injection и
  опасных инструкций, LLM-redaction PII/секретов и сканирования репозиториев.
- Краткий вывод: полезный источник контрактов, эвристик, тестовых идей и
  модели изоляции сканирования; готовой локальной подсистемой для EvoHime не
  является. Код напрямую в Core не переносить.

#### Что изучено

- `sdk/typescript/src/client.ts` и параллельная Python-реализация дают три
  операции: `guard`, `redact`, `scan`;
- `guard` принимает текст, PDF, изображения, Blob/URL, режет большие тексты на
  bounded chunks и агрегирует результат по OR: если заблокирован хотя бы один
  chunk или PDF-page, итог `block`;
- результат `guard` типизирован как `pass|block` с
  `violation_types`, `cwe_codes`, reasoning и token usage;
- провайдеры унифицированы форматом `provider/model`, поддерживают structured
  output там, где провайдер его умеет, а для superagent-моделей есть отдельный
  адаптер Ollama-style response;
- URL-fetcher проверяет только HTTP(S), запрещает credentials и localhost,
  разрешает DNS во все адреса и отклоняет любой private/internal IP, закрепляет
  проверенные адреса для соединения, повторно проверяет redirect targets,
  ограничивает 5 redirects, 30 секунд и 25 MiB;
- `scan` клонирует репозиторий в Daytona sandbox, устанавливает
  `opencode-ai`, запускает security prompt, разбирает JSONL-события и удаляет
  sandbox после выполнения;
- MCP-слой использует строгие Zod-схемы, bounded input до 50 000 символов и
  read-only/idempotent annotations для guard, redact и scan;
- исходный репозиторий содержит MIT `LICENSE`, `SECURITY.md` и тесты для
  guard, chunking, redaction, URL/SSRF и provider fallback.

#### Что можем использовать в Еве

- **Контракт оценки входа как advisory signal.** Взять структуру
  `pass|block + violation_types + cwe_codes + bounded explanation` для
  внутреннего typed-события Core и evaluation fixtures. Это может улучшить
  диагностику prompt-injection и классификацию причин, но не должно заменять
  Core policy, capability checks, approval или hard-deny.
- **Bounded chunking и консервативная агрегация.** Использовать как идею для
  bounded проверки больших внешних evidence/workspace-фрагментов: ограничить
  размер, число параллельных вызовов, бюджет и дедлайн; итог блокировать при
  одном подтверждённом опасном фрагменте, сохраняя объединённые типы нарушений.
  В EvoHime это должно проходить через уже существующий context budget и
  Core-owned execution path.
- **SSRF-safe remote fetch как эталон тестов.** Сопоставить с текущим
  `research_fetch`: полезны проверка всех DNS-адресов, DNS pinning, повторная
  проверка каждого redirect, запрет credentials/private ranges, bounded body
  и timeout. Переносить только после сверки с существующим Rust-контрактом,
  чтобы не создать вторую несовместимую сетевую политику.
- **Capability matrix для model gateway.** Таблица поддержки structured
  output/vision и provider-specific fallback полезна как источник тестовой
  матрицы для `crates/model-gateway`. Реализовывать её нужно в Rust policy
  snapshot и capability metadata Евы, а не добавлением TypeScript SDK.
- **Изолированный repository-security workflow.** Идея отдельного read-only
  sandbox, pinned revision, bounded scan и typed report подходит для будущей
  функции проверки внешнего репозитория перед подключением к Еве. Для Евы
  sandbox должен контролироваться supervisor/Core, не Daytona; результат
  должен быть untrusted evidence с provenance и redaction, а не автоматически
  подтверждённой инструкцией.
- **MCP schema/annotation pattern.** Строгие схемы, максимальная длина входа,
  read-only/idempotent metadata и явное разделение guard/redact/scan полезны
  при проектировании будущего внешнего MCP adapter или tool-manifest. Это не
  меняет текущую границу: внешний MCP требует отдельного permission/approval.
- **Тестовые сценарии.** Перенять набор классов fixtures: override system
  instructions, prompt extraction, data exfiltration, jailbreak, malicious
  repository instructions, redirect-to-private-IP, oversized response и
  malformed structured output.
- **Локальные Guard-модели как предмет оценки.** Упоминание
  `superagent-guard-0.6b/1.7b/4b` и GGUF можно использовать только для
  отдельного benchmark local provider. В репозитории нет весов и полноценного
  локального runtime, поэтому это не готовая зависимость Евы.

#### Ограничения и риски

- **Нарушение local-first при прямом подключении.** Основной SDK отправляет
  контекст во внешние Cloud Run/API endpoints или выбранному внешнему
  provider. Даже `superagentProvider` без model API key не делает обработку
  локальной.
- **Скрытая внешняя телеметрия.** `SafetyClient` требует
  `SUPERAGENT_API_KEY` уже при создании клиента и fire-and-forget отправляет
  token usage на `https://superagent.sh/api/billing/usage`. Это не подходит для
  Core-owned secrets/provenance boundary без отдельного явного opt-in.
- **Fallback расширяет egress.** При timeout SDK повторяет запрос на
  `https://superagent.sh/api/fallback`; fallback нужно рассматривать как
  отдельный provider и отдельное раскрытие данных.
- **LLM-classifier не является security boundary.** `guard` может ошибаться,
  а пустой PDF без извлекаемого текста превращается в `pass`. Поэтому его
  результат допустим только как advisory/evaluation signal; разрешение tool
  effect должно оставаться за Core policy и approval.
- **Сканирование передаёт секреты в sandbox.** `scan` собирает
  `ANTHROPIC_API_KEY` и `OPENAI_API_KEY` и передаёт их в Daytona environment,
  затем устанавливает floating `opencode-ai@latest`. Для EvoHime это
  неприемлемая модель доверия и воспроизводимости.
- **Командная поверхность scan недостаточно близка к контракту Евы.** В
  Python-пути shell-команда строится с `repo` и `branch`; перед адаптацией
  потребовались бы строгая валидация, pinned commit, отсутствие shell
  interpolation и supervisor limits.
- **Redact основан на LLM и rewrite может менять смысл.** Это подходит для
  удобного пользовательского текста, но не для доказательной redaction
  audit/provenance. В Еве сохранять deterministic redaction и typed tombstones.
- **Интеграционная зрелость неоднородна.** TypeScript SDK в ревизии собрался,
  162 теста прошли; у него отсутствует lockfile, а CLI/MCP зависят от
  опубликованного `safety-agent ^0.1.7`, тогда как SDK в checkout имеет
  `0.1.8-rc1`. Python-тесты требуют установки package/runtime dependencies;
  без `daytona_sdk` один credential test падает до проверки credentials.
- **Лицензия моделей не подтверждена лицензией кода.** MIT относится к
  исходному репозиторию; веса Guard и их условия нужно проверять отдельно,
  если benchmark когда-либо превратится в поставку.

#### Предварительное решение

`адаптировать идеи и тестовые сценарии`; `не использовать SDK/MCP/Daytona как
runtime-зависимость Евы`.

#### Связь с EvoHime

- уже покрыто и не дублировать: Core-owned redaction, context budget,
  prompt-injection envelope, model-gateway routing/capabilities, approval,
  provenance и bounded research fetch;
- возможная будущая работа: security-evaluation fixtures для внешнего
  repository/evidence scanning и typed advisory guard result;
- возможная отдельная работа после подтверждения необходимости: локальный
  guard benchmark на модели с проверенными весами и лицензией;
- критерии проверки: отсутствие нового необъявленного network egress,
  сохранение Core-only policy/approval, bounded budget/timeout/cancellation,
  redacted provenance, deterministic replay и negative tests на prompt
  injection/SSRF/secret leakage.

## Итог для будущего плана

Этот раздел заполняется после завершения набора исследований:

- подтверждённые возможности для интеграции;
- идеи, которые реализуем самостоятельно без заимствования кода;
- внешние компоненты, допустимые после проверки лицензии;
- отклонённые варианты и причины;
- зависимости, порядок этапов и критерии готовности.
