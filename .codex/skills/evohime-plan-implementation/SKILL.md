---
name: evohime-plan-implementation
description: Полностью реализовать и закрыть numbered implementation plan в EvoHime от ревью до task-only commit без локальных тестов, сборок и push.
---

# Полная реализация и закрытие плана EvoHime

## Когда применять

Применяй для запроса «полностью реализовать и закрыть план», когда требуется
провести план через ревью, код, интеграцию, release routing, документацию,
удаление plan-файлов и коммит. Для одного только ревью используй
`evohime-plan-review`; для одного только аудита — `evohime-implementation-audit`.
Короткий канонический prompt с маршрутизацией skills находится в
`../../prompts/implement-and-close-plan.md`.

## Жёсткие границы

- Текущий пользовательский запрос имеет приоритет над общими рекомендациями
  `AGENTS.md`. Если в запросе запрещены локальные тесты, сборки, линтеры,
  smoke-тесты и иные проверочные команды, не запускай их, даже если общие
  правила проекта допускают локальные проверки.
- Не создавай ветку, не меняй ветку и не выполняй push без отдельного прямого
  запроса. Работай в текущей `main` и зафиксируй расхождение с remote, если оно
  есть.
- Не включай в коммит посторонние пользовательские изменения. Не изменяй
  установленный клиент EvoHime, production-состояние или installer worker.
- Не утверждай результат по предположению. Каждый закрытый пункт должен иметь
  ссылку на код, тест/CI evidence, runtime wiring или канонический документ.

## Маршрут

1. Загрузи `evohime-repo-orientation` и выполни его read-only baseline. До
   изменений зафиксируй commit, ветку, состояние working tree и отношение
   `HEAD` к `origin/main`.
2. Найди полный комплект `docs/plans/<NN>-0` ... `<NN>-4`, прочитай `docs/README.md`,
   `docs/plans/README.md`, `docs/architecture.md`, `docs/current-state.md`,
   `docs/release-evidence.md`, `SECURITY.md` и связанные документы. Не считай
   только overview достаточным каноническим планом.
3. Загрузи `evohime-plan-review`. Проведи итерации: требования → фактический
   код и контракты → замечания → обоснованные правки плана → повторная проверка.
   Спорное замечание разрешай по источнику истины и записывай причину
   отклонения в рабочем результате ревью.
4. Загрузи `evohime-implementation-audit`. Составь coverage matrix для каждого
   пункта этапов `0–4`: source paths, migration/storage, runtime/recovery,
   IPC/preload/renderer, CI, packaging, security, rollback и docs. Исправляй
   обнаруженные пробелы в реализации до перехода к закрытию.
5. Реализуй согласованный план сквозным образом. Сохраняй архитектурную
   границу: runtime/state/permissions принадлежат Rust Core, renderer получает
   только типизированную проекцию, а изменения IPC обновляют обе стороны и
   regression contracts. Для SQLite сохраняй transactional migration и backup;
   для runtime — recovery/idempotency/unknown-outcome semantics, если они
   предусмотрены планом.
6. Загрузи `evohime-ci-evidence` и проверь реальные workflow paths, job gates,
   доступные GitHub runs и историческое evidence. Не подменяй отсутствующий
   CI локальным запуском.
7. Загрузи `evohime-module-release-routing`. После всех исходных изменений
   отдели source changes публикуемых модулей от docs/tests/CI-only changes,
   проверь живую карту workflow и подними только patch-часть нужных
   `release-versions/<module>.txt`.
8. Загрузи `evohime-documentation-closure`. Перенеси устойчивый контракт в
   `docs/architecture.md`, подтверждённое состояние в `docs/current-state.md`,
   release/verification evidence в `docs/release-evidence.md`; обнови карту
   документов и индекс планов, удали все этапы закрытого плана и проверь ссылки.
9. Загрузи `evohime-static-verification`. Выполни только разрешённые
   read-only/static проверки и `git diff --check`; не запускай project tests,
   build, lint, smoke, package или runtime.
10. Загрузи `evohime-task-only-commit`. Проверь полный diff, список staged paths,
    версии и отсутствие посторонних файлов; создай task-only commit в текущей
    ветке. После коммита повторно проверь hash, branch, status и то, что push не
    выполнялся.

## Условия готовности

Готово только когда одновременно доказаны: план отревьюирован; все его этапы
реализованы и связаны с runtime; обязательные тесты и gates либо подтверждены
доступным CI, либо честно отмечены как pending; версии изменённых source
модулей обновлены; контракт/state/evidence перенесены; plan-файлы удалены;
устаревшие ссылки устранены; `git diff --check` пройден; создан task-only
commit. В финале перечисли результат ревью, изменения, модули со старыми и
новыми версиями, выполненные без локального тестирования проверки, документы,
удалённые файлы, hash, ожидаемый post-push module-router effect и ограничения.
