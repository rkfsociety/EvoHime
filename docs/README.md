# Документация EvoHime

Этот каталог описывает поддерживаемый Windows desktop-продукт. Главный источник команд запуска и требований — корневой [`README.md`](../README.md).

## Какой документ читать

| Задача | Документ | Назначение |
| --- | --- | --- |
| Понять границы и процессы | [`architecture.md`](architecture.md) | Runtime, оболочка, IPC, данные и упаковка |
| Узнать, что уже сделано | [`current-state.md`](current-state.md) | Подтверждённое состояние checkout |
| Понять ближайший порядок работ | [`plans/00-development-plan.md`](plans/00-development-plan.md) | Исполняемый план и критерии готовности |
| Посмотреть долгосрочные направления | [`plans/99-roadmap.md`](plans/99-roadmap.md) | Крупные продуктовые этапы без деталей реализации |
| Проверить границы безопасности | [`../SECURITY.md`](../SECURITY.md) | Угрозы, доверие, диагностика и релизные проверки |

## Справочные разделы

- [`features/`](features/) — отдельные контракты и описания функций агента:
  - [`extended-reasoning.md`](features/extended-reasoning.md) — extended reasoning в Core и model gateway;
  - [`reflection.md`](features/reflection.md) — reflection loop после tool-вызовов;
  - [`task-dependency-graphs.md`](features/task-dependency-graphs.md) — графы зависимостей задач;
- [`providers/`](providers/) — провайдеры моделей и их настройка;
- [`security/`](security/) — расширенная модель угроз;

## Планы

Все планы живут в каталоге [`plans/`](plans/):

- [`plans/00-development-plan.md`](plans/00-development-plan.md) — исполняемый план текущего цикла;
- [`plans/99-roadmap.md`](plans/99-roadmap.md) — долгосрочные направления без деталей реализации.

Новый план создавайте файлом в `plans/`, а не в корне `docs/`. Параллельный статус реализации в планах не дублируется: подтверждённое состояние остаётся в [`current-state.md`](current-state.md).

## Пользовательская модель

Продукт — один локальный Windows EXE-клиент. Пользователь скачивает `EvoHime-Setup.exe`, устанавливает приложение и запускает один ярлык `EvoHime`. Короткое имя агента — **Ева**. `evohime-core.exe` и `evohime-supervisor.exe` являются скрытыми внутренними компонентами runtime.

## Правило источника истины

Если документы расходятся, приоритет такой: код и тесты → `current-state.md` → `architecture.md` → `plans/00-development-plan.md` → `plans/99-roadmap.md`. Историю решений и незавершённые варианты не переносите в статус продукта без подтверждения реализацией.

## Владельцы информации

| Вопрос | Канонический документ |
| --- | --- |
| Что реально собрано и проверено в checkout | [`current-state.md`](current-state.md) |
| Какой runtime является утверждённой целью | [`architecture.md`](architecture.md) |
| В каком порядке выполнять ближайшую работу | [`plans/00-development-plan.md`](plans/00-development-plan.md) |
| Долгосрочные направления без пошаговой реализации | [`plans/99-roadmap.md`](plans/99-roadmap.md) |
| Security boundaries и release security gates | [`../SECURITY.md`](../SECURITY.md), [`security/`](security/) |
| Provider-specific configuration | [`providers/`](providers/) |
| Core crate contracts and implementation notes | соответствующий `crates/*/README.md` |

Один факт не должен поддерживаться вручную в нескольких статусных документах:
ссылки на факт допустимы, копирование таблиц состояния — нет.

## Рабочие правила

Для текущей разработки используйте `start-dev.ps1`, native package tests, Electron checks, WinUI/IPC compatibility tests и Windows CI. Electron shell живёт в `desktop/evohime-electron`; его protocol check, typecheck, unit-, contract- и real-Core E2E тесты запускаются через npm-команды и входят в Windows CI. Установщик и пользовательский запуск работают через Electron `EvoHime.exe`.

Веб-панель полностью выведена из продукта. Electron renderer — встроенный desktop UI; HTTP server и browser launcher не используются. `start-dev.ps1` собирает native package и открывает Electron `EvoHime.exe`; клиент сам запускает единственный скрытый supervisor, а supervisor — Core. `-SkipBuild` допустим только при наличии готового `.evohime-native\windows-x64`.

При изменении архитектуры, runtime-контрактов или статуса реализации обновляйте соответствующий канонический документ и дату состояния. Не дублируйте подробный план в `plans/99-roadmap.md` и не добавляйте инструкции для отдельного web-продукта.
