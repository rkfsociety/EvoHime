# EvoHime — Windows desktop roadmap

Это краткая продуктовая карта, а не список отдельных задач. Детали текущего цикла находятся в [`development-plan.md`](development-plan.md), фактическая реализация — в [`current-state.md`](current-state.md).

Актуальный roadmap описывает один локальный Windows-клиент Ева, распространяемый через `EvoHime-Setup.exe`. Пользователь запускает один ярлык `EvoHime`; внутренние Core и supervisor не являются отдельными продуктами.

## Установочный канал

Оболочка — Electron. Установщик публикуется в одном постоянном релизе
`installer`, а актуальные commit и ветка сборки записываются в
`evohime.build.json`; дальнейшие обновления выполняются из исходников.

## Ближайшая работа

Основные runtime-направления уже реализованы. Roadmap теперь ограничен
поддержкой и hardening, а не повторным описанием закрытых планов:

### 1. Reliability и security hardening

- улучшать отображение approval, recovery и bounded rollback evidence;
- развивать credential, backup/restore и diagnostic UX в существующих границах;
- проверять upgrade path на поддерживаемых Windows 10 и Windows 11.

### 2. Desktop quality и совместимость

- сохранять Electron/Core IPC tests для каждого изменения IPC;
- поддерживать installer/package smoke, single-instance и Job Object checks;
- поддерживать bounded logs, event replay и retention без возврата web runtime;
- выполнять informative ARM64/Insider runs без изменения базового x64 release scope.

## Release workflow

1. Push или pull request запускает автоматический workflow «Быстрые проверки
   EvoHime» с быстрыми Rust/Electron gates.
2. Через `Run workflow` запускается «Ручной выпуск EvoHime»: полный Rust,
   Electron, package, installer и Windows acceptance.
3. Публикация постоянного installer release выполняется только в ручном
   workflow после успешного полного прогона.
4. Ручной запуск workflow после зелёной сборки обновляет единственный постоянный release `installer` и его `EvoHime-Setup.exe`.
5. Новые версионные релизы и теги для этого цикла не создаются.

Закрытые этапы не дублируются здесь; фактическое состояние хранится в [`current-state.md`](current-state.md), а пошаговые работы — в [`development-plan.md`](development-plan.md).
