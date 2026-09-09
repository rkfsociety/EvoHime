---
name: evohime-plan-review
description: Провести итерационное evidence-based ревью и исправление канонического implementation plan EvoHime до устранения важных пробелов и противоречий.
---

# Ревью и ревизия плана

## Область

Используй для полного набора `docs/plans/<NN>-0` ... `<NN>-4`. Сначала прочитай
реальный текст файлов и индекс планов, затем сопоставь план с живым checkout.
Старые номера, память и похожие планы не заменяют проверку текущих файлов.

## Review matrix

Для каждого требования зафиксируй:

| Область | Что подтвердить |
| --- | --- |
| Scope | цель, non-goals, граница ответственности и отсутствие дубликата |
| Dependencies | blocking/optional, порядок `0 → 1 → 2 → 3 → 4`, запрет поздней blocking-зависимости |
| Core/storage | crate-владелец, schema/migration, bounds, backup, rollback, idempotency |
| Runtime | startup, scheduling, lease/fencing, retry, crash recovery, unknown outcome |
| IPC/UI | proto tags, auth, sequence/replay, generated types, main/preload/renderer projection |
| CI/package | workflow paths, jobs, artifact, module release, installer/compatibility gates |
| Security | trust boundary, secrets, permissions, audit, redaction, fail-closed behavior |
| Evidence | deterministic acceptance criteria, evidence location, unavailable gates and reason |

## Итерация

1. Выпиши конкретные objections с path/heading и evidence, а не общие советы.
2. Проверь каждое objection по коду, тестам как тексту, workflow и каноническим
   документам. Раздели confirmed, stale, speculative и contradictory.
3. Исправляй в плане только подтверждённые omissions, unsafe assumptions,
   неверные зависимости, невозможные критерии и противоречия с источником
   истины. Не расширяй scope догадками.
4. Повтори полный matrix после каждой содержательной правки. Остановись только
   когда не осталось важных unresolved objections; для отклонённых спорных
   замечаний сохрани краткую причину, основанную на source of truth.

При запрете локальных проверок не запускай reviewer scripts, tests, builds,
linters или smoke. Разрешены чтение исходников, workflow, существующих CI
результатов, `rg`, `git diff --check` и просмотр diff.

## Выход

Результат должен содержать принятую/отклонённую матрицу замечаний, изменённые
плановые контракты и список оставшихся внешних блокеров. Само ревью не закрывает
план и не удаляет его файлы.
