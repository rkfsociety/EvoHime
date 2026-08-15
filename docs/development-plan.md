# План разработки EvoHime Desktop

Статус: исполняемый план текущего desktop-цикла. Для фактического состояния используйте [`current-state.md`](current-state.md), для долгосрочных направлений — [`roadmap.md`](roadmap.md).

## Цель

Создать стабильный локальный Windows AI-agent. Пользователь запускает desktop app, выбирает workspace, запускает задачу и получает поток событий через named pipe.

Текущая версия клиента: `0.0.000033`.

## Стек

| Слой | Технология |
| --- | --- |
| UI | Electron + TypeScript (bundled desktop renderer) |
| Core | Rust |
| IPC | versioned protobuf over Windows named pipes |
| Storage | SQLite + transactional migrations |
| Lifecycle | Rust supervisor + mutex + Job Object |
| Diagnostics | JSONL logs + replayable event journal |
| Packaging | x64 Windows package + Inno Setup installer |

## Активный цикл

Закрытые foundation, Electron shell, developer tools, installer, update, recovery и
Windows acceptance не являются задачами текущего плана. Их фактическое состояние
зафиксировано в [`current-state.md`](current-state.md).

Проверка 2026-08-14: Rust, Electron, C#/WinUI compatibility, protocol, bundle,
native, deterministic evaluation и security smoke checks прошли без ошибок;
source-update E2E остаётся штатно пропущенным без включённого флага.

Текущий цикл завершён: обходы permission policy и approval закрыты и проверены.

## Текущий статус

Legacy web UI, browser launcher, HTTP server и PostgreSQL persistence удалены. Rust
Core + SQLite + authenticated named-pipe IPC сохраняются. Пользовательская оболочка
— Electron; WinUI — compatibility runtime.

Активных задач текущего implementation-цикла нет. Следующие задачи выбираются из
долгосрочного [`roadmap.md`](roadmap.md) после отдельного подтверждения приоритета.

После завершения foundation добавлен функциональный slice «Ревью планов» в
Electron: additive desktop IPC, `plan_review` в Core и event journal для
истории. Slice доведён до рабочего состояния: последовательные reviewer calls с
прогрессом и явным статусом ошибок, стабильный состав и порядок рецензентов,
копирование и экспорт итогового Markdown, очистка истории через
`ClearPlanReviewHistory`. DOCX/PDF import, пользовательские критерии и
ZIP-экспорт остаются последующими улучшениями.

## Acceptance criteria

- запуск с ярлыка не открывает браузер и консоль;
- UI и core эволюционируют независимо через IPC versioning;
- перезапуск core не теряет завершённые события;
- отмена задачи завершает дочерние процессы;
- опасные операции требуют approval и показывают preview;
- обновление восстанавливает компоненты из pre-upgrade backup при ошибке и после аварийного завершения;
- core tests работают без UI-сессии, Electron smoke и packaging acceptance — на Windows CI.

При расхождении этого плана с реализацией сначала обновляется статус на основании кода и тестов. В `docs/plans/` хранятся только временные планы реализации; этот документ описывает исполняемый цикл, а долгосрочные направления — [`roadmap.md`](roadmap.md).
