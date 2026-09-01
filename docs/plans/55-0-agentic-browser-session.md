# План 55.0 — Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой

Статус: предложено по [issue #35](https://github.com/rkfsociety/EvoHime/issues/35). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Agentic Browser Session**: управляемую браузерную сессию для задач, где агенту нужно видеть и взаимодействовать с web UI, но без выдачи renderer/model прямого доступа к CDP, произвольному локальному браузерному профилю или unrestricted network socket.

Browser runtime должен давать модели стабильные typed references на страницу/элементы и проходить через обычные Core capabilities, approvals, execution/network policy и event logging.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Live checkout уже содержит `tool-runtime/src/tools/browser_session.rs`,
`cdp.rs`, `ssrf.rs` и зарегистрированные `browser.session.*` tools. План не
создаёт параллельный browser stack: он заменяет env-supplied raw CDP/CSS
selector contract на Core-owned lifecycle, page-revision-bound element refs,
isolated packaged profile и policy-checked backend adapter. Existing one-shot
browser tools остаются отдельной capability.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./55-1-agentic-browser-session.md)
- [Этап 2 — runtime-интеграция и recovery](./55-2-agentic-browser-session.md)
- [Этап 3 — IPC, client projection и UI](./55-3-agentic-browser-session.md)
- [Этап 4 — verification, release-evidence и закрытие](./55-4-agentic-browser-session.md)

## Зависимости

### Блокирующие

- действующие ToolRegistry/permission/approval, `browser.session.*`, CDP и SSRF
  contracts как migration baseline;
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- Execution Policy Profiles v1 может задавать дополнительные backend limits;
  без отдельного profile используются строгие browser defaults.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- Core владеет session identity и tool routing;
- browser не расширяет grants;
- SSRF/private-network policy применяется на navigation/redirect/DNS resolution;
- isolated profile default;
- raw browser credentials не попадают в model/renderer trace;
- element refs scoped к session/page revision;
- arbitrary host file upload запрещён;
- download не выполняется;
- browser internal/file/custom schemes deny-by-default;
- renderer не получает direct CDP endpoint.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Обязательные решения definition freeze

- Единственным authoritative owner является `evohime-core`: `tool-runtime` не
  хранит самостоятельный registry/lifecycle, а получает bounded typed command
  от Core. Существующие `browser.session.*` имена с raw selector и
  `EVOHIME_BROWSER_CDP_URL` не считаются совместимым API: до включения новой
  capability они отключаются или переводятся в typed `legacy_disabled`, без
  fallback к старому пути.
- Backend — Core-launched packaged Chromium/CDP adapter с отдельным
  EvoHime-owned profile и Job Object/lifecycle cleanup; arbitrary external CDP
  endpoint и личный профиль не входят в production contract. Если backend не
  входит в текущий package, capability остаётся typed `unavailable`, а stage не
  закрывается критериями runtime/UI.
- Network enforcement выполняется backend-ом на каждом redirect и перед
  соединением с каждым разрешённым IP; одной DNS-проверки исходного URL
  недостаточно. Core policy snapshot задаёт режим и allowlist, default
  `PublicInternet` блокирует private/link-local/metadata/localhost и все
  non-http(s)/browser-internal/file/custom schemes.
- Все page/element refs ephemeral и включают session id, page revision и
  fingerprint; screenshot/page text/download являются ArtifactStore objects с
  bounded locator/hash. Прямая запись browser tool в workspace и host-path из
  model input запрещены. Download только staging+artifact, execution
  отсутствует; upload только по разрешённому artifact/workspace ref.
- Human takeover — Core-owned exclusive lease/generation. Agent mutations
  отклоняются во время lease, release создаёт новую revision/snapshot, а
  stale refs получают typed error.

## Критерии готовности из issue

- [ ] Есть Core-owned BrowserSession lifecycle.
- [ ] Модель работает через typed browser tools и stable element refs.
- [ ] Refs имеют page revision и stale protection.
- [ ] Есть network/SSRF policy с private-address protection.
- [ ] Default browser profile isolated/ephemeral.
- [ ] Upload/download проходят Artifact/Core boundaries.
- [ ] Workbench может показывать безопасную live projection.
- [ ] Human takeover поддерживается без гонки с agent actions.

## Ограничения и non-goals

- stealth/anti-bot обход;
- CAPTCHA solving;
- использование личного browser profile без explicit opt-in;
- unrestricted CDP для модели/renderer;
- автоматический запуск downloads;
- обход сайтовых permission/security механизмов;
- полноценный replacement обычного пользовательского браузера.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#35 Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой](https://github.com/rkfsociety/EvoHime/issues/35)

## Результат ревью 2026-09-01

- Учтена уже существующая CDP/browser-session реализация; план теперь является
  её безопасной миграцией, а не вторым browser runtime.
- На definition freeze вынесены redirect/DNS rebinding, stable refs, isolated
  profile, ArtifactStore download/upload и human-takeover fencing.
