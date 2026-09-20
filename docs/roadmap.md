# EvoHime — roadmap

Обновлено: 2026-09-20.

Это краткая продуктовая карта, а не список отдельных задач. Исполняемый порядок
находится в [`development-plan.md`](development-plan.md), подтверждённое
состояние — в [`current-state.md`](current-state.md), а детализация очереди — в
[`plans/README.md`](plans/README.md).

Текущая активная очередь содержит незавершённые implementation contracts
`173–181`; планы `01–172` перенесены в canonical docs.

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
- поддерживать self-healing существующего updater и control-plane recovery без
  нового модуля или второго update channel.

### 2. Desktop quality и совместимость

- сохранять Electron/Core IPC tests для каждого изменения протокола;
- поддерживать package, installer, update-gate и fault-recovery smoke checks;
- сохранять bounded logs, event replay и retention без возврата web runtime;
- выполнять informative ARM64/Insider runs без изменения базового x64 release scope.

### 3. Модульные релизы компонентов

Компонентный манифест, выборочная транзакция, UI bundle и recovery являются
текущим базовым контрактом поставки; детали описаны в
[`architecture.md`](architecture.md), а проверки — в
[`release-evidence.md`](release-evidence.md).

Этот контракт не разрешает удалённый control plane, автоматический
self-repair, обход approval или изменение установленного клиента.

## Ограничения roadmap

- локальный Windows-first release остаётся базовым продуктом;
- новые provider adapters и дополнительные платформы не становятся
  блокирующими для базового пакета без отдельного принятого решения;
- история закрытых планов хранится в release evidence, а не дублируется здесь.

## Release workflow

1. Push или pull request запускает модульный workflow и быстрые проверки
   затронутых областей.
2. Центральный `module-router` dispatch’ит workflow затронутых модулей и
   быстрый web-installer workflow; native package acceptance остаётся только
   ручной диагностической проверкой и не входит в release path. Результат
   фиксируется в `release-evidence.md`.
3. Единственный release `installer` обновляется после зелёных module и
   compatibility gates; новые версионные теги текущим циклом не создаются.
4. Локально по умолчанию выполняются быстрые проверки; полный прогон при
   необходимости выполняется в GitHub Actions как обязательный acceptance
   source.

Карта workflow и команды проверки находятся в [`../AGENTS.md`](../AGENTS.md) и
`.github/workflows/`.
