# EvoHime — Windows desktop roadmap

Это краткая продуктовая карта, а не список отдельных задач. Детали текущего цикла находятся в [`development-plan.md`](development-plan.md), фактическая реализация — в [`current-state.md`](../current-state.md).

Актуальный roadmap описывает один локальный Windows-клиент Ева, распространяемый через `EvoHime-Setup.exe`. Пользователь запускает один ярлык `EvoHime`; внутренние Core и supervisor не являются отдельными продуктами.

## Текущая версия

`0.0.000033` — текущая версия клиента; оболочка — Electron; первая версия продукта была `0.0.0001`.

## Завершено

| Блок | Статус | Подтверждение |
| --- | --- | --- |
| WinUI 3 shell и native foundation | ✅ legacy baseline | `bb432fa` |
| Electron desktop shell migration | ✅ Windows acceptance закрыта | `b3187e9`, `0afc33d` |
| Rust Core и SQLite event journal | ✅ | `93995bc`, `66e741e` |
| Versioned named-pipe IPC и replay | ✅ | `e0da370`, `463e11b` |
| Supervisor, mutex, Job Object и diagnostics | ✅ | `e0e0f75`, `a9018a8` |
| Workspace picker, persistence, tray, notifications | ✅ | `a43aaac`–`6991a11` |
| Desktop task timeline, cancellation и approval round-trip | ✅ Core/legacy UI baseline | `0246f05`, `87c5b39` |
| Единый installer и CI build after checks | ✅ | `9b3430c` |
| Имя агента «Ева» и версия `0.0.0001` | ✅ | `775b20b` |
| Retention: только последний стабильный release/tag | ✅ | `dadcbf6` |
| Автообнаружение обновления, SHA-256 проверка, upgrade smoke и автоматический rollback | ✅ | `edaa8ec` |
| Hardening секретов, backup/restore и recovery UI | ✅ | `5033356` |
| Продуктовая оболочка: проекты и чаты, главный экран, читаемая лента задачи, выбор модели | ✅ | `a403dec` |

## Ближайшая работа

### 1. Reliability and approval UX — следующий продуктовый этап

- leases/reconciliation для длительных запусков;
- расширенный diff/command preview в approval UI;
- дальнейшее закрытие обходов permission policy и безопасных child contracts.

### 2. Reliability and security hardening

- расширение Windows Credential Manager/DPAPI и backup/restore UX;
- crash recovery и диагностика из UI;
- проверка upgrade path на поддерживаемых Windows 10 и Windows 11.

### 3. Desktop quality

- compatibility tests UI/Core для каждого изменения IPC;
- smoke installer на Windows CI;
- проверка single-instance и завершения Job Object;
- bounded logs, event replay и retention completed tasks;
- release только после зелёных Rust/Electron/package checks и Windows acceptance.

## Release workflow

1. Push или pull request запускает проверки Rust, supervisor, Electron, package smoke и Windows acceptance.
2. Job `build-native` стартует только после успешных проверок.
3. Собирается runtime в staging-каталог.
4. Inno Setup создаёт единственный `EvoHime-Setup.exe`.
5. Для tag `vX.Y.Z` создаётся GitHub Release.
6. Еженедельная retention-задача удаляет все versioned Releases/tags, кроме последнего.

Миграция Electron закрыта на Windows и не считается активным планом. Пошаговые работы текущего цикла ведутся в [`development-plan.md`](development-plan.md) и намеренно не повторяются в roadmap.
