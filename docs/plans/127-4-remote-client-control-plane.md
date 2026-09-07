# План 127.4 — Remote Client Control Plane: verification, release evidence и закрытие

Статус: этап 4 для [плана 127.0](./127-0-remote-client-control-plane.md); issue: [#108](https://github.com/rkfsociety/EvoHime/issues/108).

## Зависимости

### Блокирующие

- План 127.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Сформировать focused relay/parser/installer, storage, runtime/recovery, IPC/replay/redaction и Android accessibility checks. Проверить clean/repeat/interrupted installer, least-privilege service, token replay/revoke, IP-only access, TLS downgrade, malformed/oversized frames, multiple clients, changing client IP и end-to-end Android → static-IP relay → connector → Core.

Измерить relay CPU, память, sockets, traffic и latency для 1, 10 и согласованного максимума sessions; фиксировать build/network conditions и p50/p95/p99, не подменяя baseline неподтверждёнными целями. Отдельно проверить, что текущая SECURITY.md не объявляет публичный relay уже поддержанным до завершения работ. После реализации и security review обновить SECURITY.md, затем перенести подтверждённый контракт в docs/architecture.md, состояние в docs/current-state.md и release procedure в docs/release-evidence.md; выполнить git diff --check и удалить полный комплект только по правилам.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.
- [ ] Security negative tests доказывают, что IP, чужой APK, replay и cross-device token не дают доступа.
- [ ] Root credentials, raw prompts, provider secrets и private keys отсутствуют в logs, APK, storage и evidence.
- [ ] Красный security/recovery gate блокирует release; rollback не удаляет Core data.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных.
