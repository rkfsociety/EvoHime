# План разработки EvoHime Desktop

Статус: foundation, desktop shell, automation, self-repair/self-update и
технические release-gates реализованы. Пользовательский self-repair/self-update
включён как строго ручной production-контур; автоматический ремонт и
автоматический push не входят в продукт. Фактическое состояние checkout находится в
[`current-state.md`](current-state.md), архитектурные контракты — в
[`architecture.md`](architecture.md), а долгосрочные направления — в
[`roadmap.md`](roadmap.md).

## Цель текущего цикла

Сохранить стабильный локальный Windows AI-agent: пользователь запускает один
desktop-клиент, выбирает workspace, выполняет задачу и получает поток событий
через authenticated versioned named pipe. Core остаётся владельцем состояния,
прав доступа, эффектов и SQLite; Electron отображает только IPC-проекцию.

Пользовательские версионные релизы для текущего цикла не создаются. Постоянный
релиз `installer` определяется коммитом и веткой в `evohime.build.json`.

## Текущий порядок работ

Обзор и этапы текущего цикла оформлены в
[`plans/22-0-reliability-security-hardening.md`](plans/22-0-reliability-security-hardening.md).

1. **Поддержка релиза.** Сохранять зелёными Rust/Electron/package gates и
   Windows compatibility/installer acceptance — этап 22.3.
2. **Reliability и security hardening.** Улучшать credential, recovery,
   diagnostics и backup/restore UX без расширения полномочий renderer — этапы
   22.1 и 22.2.
3. **Совместимость.** Поддерживать Windows 10/11 CI; informative ARM64/Insider
   runs остаются исследовательскими и не меняют базовый release scope — этап
   22.3.
4. **Продуктовая граница.** Не возвращать web runtime, public HTTP, внешний
   Node/Python runtime или автоматические repair/push/restart действия.

Владельцы, критерии закрытия и влияние на выпуск находятся в
[`decision-register.md`](decision-register.md). Порядок работ не меняет
границы продукта: внешний cloud control plane, public HTTP, обязательный GPU,
внешний Node/Python runtime и unrestricted adapter fallback не добавляются.

## Критерии готовности

- Rust Core, storage, desktop IPC и supervisor проходят свои тесты и проверки
  формата;
- Electron `check:protocol`, `typecheck`, unit/contract tests и bundle checks
  проходят;
- automation boundary и release evidence gates проходят без credentials и
  необезличенных данных;
- Windows package, installer, upgrade, rollback и compatibility smoke проходят
  в CI;
- каждый закрытый open decision имеет код, focused test, redacted evidence и
  обновлённые `current-state.md`, `decision-register.md` и `release-evidence.md`;
- `git diff --check` проходит, а task-only изменения зафиксированы коммитом.

## Правило обновления документов

При расхождении сначала проверяются код и тесты, затем обновляется
[`current-state.md`](current-state.md). Архитектурные изменения фиксируются в
[`architecture.md`](architecture.md), решения — в
[`decision-register.md`](decision-register.md), а статус выпуска — в
[`release-evidence.md`](release-evidence.md). Исторические результаты не смешиваются
с текущей проверкой: для них указываются дата, команда и область проверки.
