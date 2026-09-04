# EvoHime — roadmap

Обновлено: 2026-09-04.

Это краткая продуктовая карта, а не список отдельных задач. Исполняемый порядок
находится в [`development-plan.md`](development-plan.md), подтверждённое
состояние — в [`current-state.md`](current-state.md), а детализация очереди — в
[`plans/README.md`](plans/README.md).

## Текущий продукт

EvoHime — один локальный Windows-клиент Ева, распространяемый через постоянный
`EvoHime-Setup.exe`. Пользователь выбирает workspace, чат, provider и model;
получает поток событий и approval в Electron shell. Core и supervisor остаются
внутренними компонентами.

## Направления

### 1. Reliability и security hardening

- улучшать отображение approval, recovery и bounded rollback evidence;
- развивать credential, backup/restore и diagnostic UX внутри текущих границ;
- поддерживать authenticated Core startup, single-instance и Job Object checks;
- проверять upgrade path на поддерживаемых Windows 10 и Windows 11.

### 2. Desktop quality и совместимость

- сохранять Electron/Core IPC tests для каждого изменения протокола;
- поддерживать package, installer, update-gate и fault-recovery smoke checks;
- сохранять bounded logs, event replay и retention без возврата web runtime;
- выполнять informative ARM64/Insider runs без изменения базового x64 release scope.

### 3. Модульные релизы компонентов

План 144 реализован: компонентный манифест, выборочная транзакция, UI bundle и
recovery описаны в [`architecture.md`](architecture.md) и подтверждены в
[`release-evidence.md`](release-evidence.md).

План 144 не разрешает удалённый control plane, автоматический self-repair,
обход approval или изменение установленного клиента в рамках текущего
checkout.

## Ограничения roadmap

- локальный Windows-first release остаётся базовым продуктом;
- новые provider adapters и дополнительные платформы не становятся
  блокирующими для базового пакета без отдельного принятого решения;
- история закрытых планов хранится в release evidence, а не дублируется здесь.

## Release workflow

1. Push или pull request запускает workflow быстрых Rust/Electron проверок.
2. Ручной workflow выпускает полный Rust, Electron, package, installer и
   Windows acceptance набор.
3. Постоянный release `installer` обновляется только после зелёного полного
   прогона; новые версионные теги текущим циклом не создаются.

Карта workflow и команды проверки находятся в [`../AGENTS.md`](../AGENTS.md) и
`.github/workflows/`.
