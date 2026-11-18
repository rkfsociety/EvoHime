# План 196.4 — документация, evals, тесты и release evidence

Статус: active implementation contract. Этап следует после 196.3 и закрывает план 196.

## Scope и изменяемые контракты
1. Документация канонического лексикона:
   - `docs/agent-tool-playbook.md` — таблица форматов вызовов с underscore-id;
   - `docs/architecture/capabilities.md` — заменить описание пары
     `provider_function_name`/`canonical_tool_call_name` на правило «id равно
     wire-имени», объяснить отличие tool id от `capability_class`;
   - `docs/current-state.md` — подтверждённое состояние после переименования;
   - `docs/providers/openai-compatible-profiles.md` — раздел о замене точек
     переписать: трансляции больше нет, потому что id изначально provider-safe;
   - `crates/tool-runtime/README.md` — перечень id;
   - `docs/release-evidence.md` — evidence этапа.
2. Актуализировать планы `191-*` и `192-*`, где id названы вместе с номерами
   строк `manifest.rs` и `legacy_parser.rs`: строки сдвинутся, ссылки должны
   остаться рабочими.
3. Evals и фикстуры: `tests/evals/fixtures/**` (5 файлов) и описательные
   `forbidden_tool_calls`/`required_tool_calls`; вымышленное имя
   `filesystem.read_outside_workspace` остаётся как есть — оно не резолвится в
   реестр.
4. Электронные тесты: 9 TS-файлов с точными id (`transcript`, `activity-line`,
   `task-timeline`, `chat-summary-panel`, `adaptive-tool-catalog-panel`,
   `trace-panel`, `continuation-panel`, `receipt-runtime-crypto`,
   `pinned-goal-bar`).
5. Удалить переходные комментарии «заменяем точки перед отправкой» и любые
   упоминания временного алиаса, кроме раздела про legacy-приём.
6. После подтверждения CI перенести контракт в доменный файл `docs/architecture/` и
   `current-state.md`, удалить файлы `196-0..196-4` и строку плана из
   `docs/plans/README.md`.

## Зависимости
### Блокирующие
196.1–196.3; зелёные CI по `tool-runtime`, `evohime-core`, `evohime-receipts`,
`evohime-local-storage`, `model-gateway`, Electron.
### Опциональные
Планы 193–195 могут ссылаться на id в тексте; их правка не блокирует закрытие
196, если поведение не затронуто.

## Recovery и rollback
Документация правится после кода, а не до него: контракт описывает уже
проверенное поведение. Если CI красный, этап не закрывается и план остаётся в
каталоге; удаление плана без evidence запрещено.

## Verification
- `git diff --check` и проверка внутренних ссылок документации.
- Полный набор CI: `tool-runtime`, `evohime-core`, `evohime-receipts`,
  `evohime-local-storage`, `model-gateway`, Rust documentation gates,
  Electron `typecheck`/`test`/`check:protocol`.
- Выборочная проверка: ни один документ не обещает трансляцию имён.

## Release evidence
Список обновлённых документов, ссылки на успешные CI-запуски по SHA, таблица
legacy→canonical, подтверждение `git ls-remote origin main` для коммита этапа.

## Критерии выхода
- [ ] Ни один документ не описывает подмену имён как действующий механизм.
- [ ] Планы 191–195 и eval-фикстуры согласованы с каноническим id.
- [ ] CI подтверждает весь набор проверок.
- [ ] Контракт перенесён в `docs/architecture/`/`current-state.md`, план удалён.
