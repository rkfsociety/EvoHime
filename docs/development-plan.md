# План разработки EvoHime Desktop

Статус: foundation, desktop shell, планы 01–19 и технические release-gates
реализованы. Пользовательский self-repair/self-update включён как строго
ручной production-контур; автоматический ремонт и автоматический push не
входят в продукт. Фактическое состояние checkout находится в
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

## Закрытые направления

Планы 01–21 завершены. Их временные файлы удалены из `docs/plans/`; контракты и
подтверждённое состояние перенесены в канонические документы. Optional
browser/voice/vision adapters остаются fail-closed capability boundaries и не
являются обязательными зависимостями базового Core package.

## Текущий порядок работ

1. **O-AUTO-01 — закрыто.** Scheduler timezone/missed-tick, durable cursor,
   additive automation IPC и acceptance gates подключены.
2. **O-AUTO-02 — закрыто.** Transactional archive/restore, checksum,
   bounded restore и retention sweep покрыты focused evidence.
3. **O-LIC-01 — закрыто.** Locked Cargo/npm inventory проверяется CI gate’ом.
4. **O-SIGN-01 — принято вне scope.** Code signing не входит в текущий
   release cycle; manifest/hash остаётся документированным trust root.
5. **O-REPAIR-01 — закрыто.** Пользовательский repair-run, отдельные
   diagnose/patch, commit, push, CI и health-gated rollback реализованы;
   запуск только кнопками, исходный workspace не изменяется.
6. **План 21 — закрыт.** Reliability/recovery UX, bounded repair/update
   evidence, diagnostic bundle и backup/restore UX реализованы; Windows CI
   остаётся release-gate.

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
  обновлённые `current-state.md`, `decision-register.md` и `release-audit.md`;
- `git diff --check` проходит, а task-only изменения зафиксированы коммитом.

## Правило обновления документов

При расхождении сначала проверяются код и тесты, затем обновляется
[`current-state.md`](current-state.md). Архитектурные изменения фиксируются в
[`architecture.md`](architecture.md), решения — в
[`decision-register.md`](decision-register.md), а статус выпуска — в
[`release-audit.md`](release-audit.md). Исторические результаты не смешиваются
с текущей проверкой: для них указываются дата, команда и область проверки.
