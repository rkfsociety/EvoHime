# План 05.9 — Offline verification и export

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — сделать provenance проверяемым без доверия к работающему приложению: расширить `evohime-verify.exe` и существующий receipt export bundle.

## Зависимости

### Блокирующие

- [05.1](05-1-canonical-request-contract.md) — canonical bytes и векторы;
- [05.2](05-2-durable-storage.md) — источник данных для export;
- [05.5](05-5-receipt-and-tool-linkage.md) — signed request receipt и tool linkage;
- существующие `evohime-verify.exe` и export bundle receipts.

### Опциональные

- [05.8](05-8-redaction-and-retention.md) — состояния `redacted`/`retention_pruned`. Если 05.8 ещё не сделана, verifier проверяет только полные и повреждённые envelope; умение отличать редактированное от повреждённого добавляется вместе с ней и обязательно до выпуска.
- [05.6](05-6-compaction-shadowing.md) — без неё раздел `context_evidence/` в bundle содержит только прямые ссылки, без цепочек summary → originals.

## Verifier

Verifier должен проверять:

```text
request envelope canonical hash
signed request receipt
receipt chain linkage
source hash references
tool receipt linkage
```

Отдельно требуется различать три исхода: валидно, редактировано пользователем (`redacted`/`retention_pruned`), повреждено или не сходится по хешу. Смешивать их в один «invalid» запрещено.

## Export bundle

Export bundle может добавить versioned sections:

```text
model_requests/
context_evidence/
manifest
```

Manifest содержит минимум:

```text
schema versions
request count
receipt count
hashes
chain roots/checkpoints
```

Export atomic, bounded и без credentials.

## Тесты

### Unit

- verifier принимает валидный bundle и отвергает подменённый блок;
- три исхода различаются на подготовленных фикстурах;
- manifest соответствует содержимому bundle.

### Integration

1. **Полный путь:** export → offline verify без запущенного Core.
2. **Redacted:** редактированный envelope проходит проверку цепочки и помечается `redacted`.
3. **Повреждение:** изменённый байт payload даёт hash mismatch, а не `redacted`.

### Property tests

Для accepted envelope:

```text
reconstruct(envelope) == original logical request
```

и:

```text
hash(reconstruct(envelope)) == envelope_hash
```

Каждый required provenance ref обязан разрешаться.

## Критерии готовности

1. Offline-проверка не требует доверия к renderer и к работающему Core.
2. Bundle атомарен, ограничен по размеру и не содержит credentials.
3. Property tests зелёные на сгенерированных envelope.
