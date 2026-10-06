# 190-4 — Verification, release evidence и закрытие

## Зависимости

Blocking: [190-3](190-3-bounded-runtime-performance.md). Optional: 184/185
для дополнительной оценки; отсутствие этих планов не блокирует закрытие.

## Приёмка

1. Все correctness cases этапов 1–3 проходят; кеши и очередь имеют проверенные
   bounds, workflow не повторяет всю историю и не теряет terminal events.
2. Redaction, rotation, shutdown и наблюдаемость ошибок logger сохранены.
3. Лента сохраняет navigation/scroll/copy/retry; durable данные не удалены.
4. Сравнительные измерения используют одинаковые fixtures/build settings,
   число повторов и машину. Evidence содержит baseline/change, p50/p95 там,
   где применимо, peak bytes, IPC counts и ограничения. Отсутствие ускорения
   или unavailable замер фиксировать прямо; не подменять его статическим выводом.
5. Self-review исключает ненужные abstraction/dependencies, изменения
   installed version, permissions, SQLite/proto и несвязанный refactoring.

## Проверки и поставка

- Свежие focused Rust/Electron tests из этапов, полный Electron suite,
  `npm run typecheck`, `npm run check:protocol`, `npm run build` и
  `npm run check:bundle` по актуальным scripts; тяжёлую сборку предварительно
  обосновать проверкой изменённого production bundle.
- При изменении Rust API/docs — соответствующие documentation gates;
  Rust fmt/clippy/check и module tests по изменённой области.
- `pwsh -NoProfile -File scripts/documentation.tests.ps1`, `git diff --check`.
- Существующие module-router, shell-host/updater workflows и rustdoc CI после
  разрешённого push; не создавать workflow ради этой задачи. Для общего logger
  подтвердить path-filter coverage обоих Electron packages. Проверять exact
  commit SHA, причины skipped/failed jobs и публикацию только реально
  изменённых модулей.
- Версии повышать только для изменённых публикуемых sources; документационный
  план сам по себе не требует marker bump. Push implementation — по правилам
  `AGENTS.md`; план не даёт разрешения на deploy/обновление установленного продукта.

## Документы и закрытие

Перенести действующий контракт в `docs/architecture.md`, подтверждённое
состояние в `docs/current-state.md`, проверки в `docs/release-evidence.md`.
Обновить очередь и каталог, удалить только реализованные stage-файлы.
`.codex/memory.md` менять лишь при новом неочевидном долговременном знании,
не копировать замеры или историю работы.

## Recovery и rollback

Проверить restart/reconnect и отсутствие зависимости от transient кешей.
Rollback — task-scoped revert исходников и соответствующий module release
по текущему workflow; не откатывать пользовательские данные. Не оставлять
build/cache artifacts, если они больше не нужны. Непроверенные gates назвать
явно; без обязательного evidence направление не объявлять закрытым.
