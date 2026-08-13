# Подплан 2 — Memory v1: extraction и native UX

Статус: следующий после hardening
Порядок: 2 из 5
Источник: [evohime-master-plan.md](evohime-master-plan.md)

## Цель

Превратить готовые Memory domain, SQLite persistence, API и IPC-контракты в ограниченный пользовательский workflow без скрытой записи фактов.

## Объём

- post-run extraction фактов и решений только из bounded run evidence;
- policy для типов записей, TTL, privacy label, provenance и максимального размера;
- подтверждение пользователем важных записей до сохранения;
- native inspector UI: create, list, search, update, archive, forget, provenance;
- export/delete только через approval и audit;
- scope isolation для workspace/project/task и deterministic retrieval.

## Порядок реализации

1. Описать `MemoryCandidate` и deterministic extractor из run metrics/evidence.
2. Добавить policy decision и confirmation queue без автоматического сохранения неподтверждённых важных фактов.
3. Подключить post-run hook к Core task lifecycle после terminal outcome.
4. Добавить WinUI inspector поверх существующего IPC API.
5. Добавить integration/eval fixtures для stale, conflicting, secret-like и cross-scope записей.

## Критерии готовности

- успешный и неуспешный run создают bounded candidates, но важные записи требуют подтверждения;
- в памяти нет stdout/stderr, полного argv, секретов и абсолютных путей вне workspace;
- запись доступна только в своём scope и имеет provenance/TTL/privacy;
- archive/forget/export/delete корректно отображаются в UI и попадают в audit;
- migration rollback и offline operation проходят без потери существующих записей.

## Зависимости

Требует завершённые Memory contracts/storage wiring и желательно метрики task runner. Полный RAG/vector search не входит: это Memory v2.
