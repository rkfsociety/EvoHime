# EvoHime — Windows desktop roadmap

Это краткая продуктовая карта, а не список отдельных задач. Детали текущего цикла находятся в [`development-plan.md`](development-plan.md), фактическая реализация — в [`current-state.md`](current-state.md).

Актуальный roadmap описывает один локальный Windows-клиент Ева, распространяемый через `EvoHime-Setup.exe`. Пользователь запускает один ярлык `EvoHime`; внутренние Core и supervisor не являются отдельными продуктами.

## Установочный канал

Оболочка — Electron. Установщик публикуется в одном постоянном релизе
`installer`, а актуальные commit и ветка сборки записываются в
`evohime.build.json`; дальнейшие обновления выполняются из исходников.

## Ближайшая работа

Завершено в текущем checkout: пользовательский self-repair/self-update
контур 19.0. Он не является автоматическим агентом: накопление ошибок только
показывает кнопку, а diagnose, commit, push и restart подтверждаются отдельно.
После установки новая версия обязана пройти authenticated Core health-check;
иначе transaction worker возвращает предыдущую установку.

### 1. Reliability and approval UX

- улучшение отображения approval и recovery-состояний в desktop UI;
- отображение подробных CI check-runs и bounded rollback evidence для repair-run;

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

1. Push или pull request запускает проверки Rust, supervisor, Electron, evaluation, package smoke и Windows acceptance.
2. Job `build-native` стартует только после успешных проверок.
3. Собирается runtime в staging-каталог.
4. Ручной запуск workflow после зелёной сборки обновляет единственный постоянный release `installer` и его `EvoHime-Setup.exe`.
5. Новые версионные релизы и теги для этого цикла не создаются.

Закрытые этапы не дублируются здесь; фактическое состояние хранится в [`current-state.md`](current-state.md), а пошаговые работы — в [`development-plan.md`](development-plan.md).
