# EvoHime — Windows desktop roadmap

Это краткая продуктовая карта, а не список отдельных задач. Детали текущего цикла находятся в [`development-plan.md`](development-plan.md), фактическая реализация — в [`current-state.md`](../current-state.md).

Актуальный roadmap описывает один локальный Windows-клиент Ева, распространяемый через `EvoHime-Setup.exe`. Пользователь запускает один ярлык `EvoHime`; внутренние Core и supervisor не являются отдельными продуктами.

## Текущая версия

`0.0.000033` — текущая версия клиента; оболочка — Electron.

## Ближайшая работа

### 1. Reliability and approval UX

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
4. Inno Setup обновляет единственный постоянный release `installer` и его `EvoHime-Setup.exe`.
5. Новые versioned releases и tags для этого цикла не создаются.

Закрытые этапы не дублируются здесь; фактическое состояние хранится в [`current-state.md`](../current-state.md), а пошаговые работы — в [`development-plan.md`](development-plan.md).
