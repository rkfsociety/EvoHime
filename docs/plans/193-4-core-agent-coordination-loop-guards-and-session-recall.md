# План 193.4 — verification, release evidence и закрытие

Статус: active implementation contract. Этап следует после 193.1–193.3.

## Scope и изменяемые контракты
1. Выполнить focused tests contracts/canonicalization, loop guard, coordination, child handoff, session query, recovery и IPC/UI.
2. Проверить failure matrix: duplicate tool call, oscillation, repeated error, legitimate polling, no progress, partial trace, stale assignment, conflicting child outputs, crash/restart, query timeout и scope mismatch.
3. Проверить security: no grant escalation, no cross-workspace recall, no raw transcript/prompt leakage, no credential propagation, no blind retry.
4. Проверить concurrency/resource pressure и cancellation на bounded workloads.
5. Обновить architecture/current-state/release-evidence и каталог plans только по подтверждённым фактам; удалить временные plan files 193 после closure.

## Зависимости
### Блокирующие
Завершённые 193.1–193.3, свежие focused test evidence, существующие security/approval/task lease/resource admission и CI gates.
### Опциональные
План 184 trace evaluator и план 185 behavioral simulation — дополнительные regression suites.

## Recovery и rollback
План остаётся active, если хотя бы один security/recovery criterion не подтверждён. Rollback выключает guard/coordinator/query UI hooks, сохраняя historical records и совместимость старых клиентов. Никаких автоматических повторных side effects.

## Verification
Contract/schema tests → deterministic loop matrix → multi-agent integration and fault recovery → session-query privacy/scope matrix → IPC/renderer tests → CI and Windows acceptance. Соблюдать AGENTS.md: тяжёлые локальные сборки не запускать без прямой просьбы.

## Release evidence
Versions/hashes contracts, deterministic fixture results, redacted loop outcomes, assignment/handoff revision refs, query scope/redaction results, IPC compatibility, focused CI links и rollback outcome. Не включать prompts, chain-of-thought, secrets, transcript bodies и absolute paths.

## Критерии выхода
- [ ] Все критерии 193.0 подтверждены observed evidence.
- [ ] Canonical docs отражают фактическое поведение, не плановые намерения.
- [ ] Нет второго loop, team, session, memory, scheduler или tool owner.
- [ ] Security, recovery, compatibility и focused CI gates зелёные.
- [ ] Plan files 193 удалены после переноса канонического контракта.
