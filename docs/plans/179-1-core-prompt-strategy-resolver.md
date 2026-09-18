# План 179.1 — Profile registry, assets и storage

## Изменить

1. Add versioned `PromptStrategyProfile`, binding, lifecycle state, composition,
   sampling plan, output contract, evidence refs and canonical content hash.
2. Add bounded immutable example-set descriptors containing ordered refs,
   provenance and privacy classification. Conversation/RAG/tool data cannot
   become reusable examples without explicit validation/promotion.
3. Add strategy snapshot identity for route capability epoch, context profile,
   loadout/output contract and selection reason; one validated revision is
   immutable and promotion/supersede uses CAS.
4. Store bounded metadata through existing Core SQLite/domain facade. Do not
   create prompt database, filesystem registry or raw production prompt store.
5. Keep state/history replayable: Draft -> Validated -> Promoted -> Superseded/
   Disabled; existing runs reference exact revision/hash.

## Зависимости

### Блокирующие

- Existing context/loadout, output/tool contract, provenance and local-storage
  conventions; plan 176 recipe refs must remain additive.

### Опциональные

- Existing artifact/context references may back validated example assets; raw
  prompt content is not required for the registry MVP.

## Проверка

Test canonical serialization/hash, bounded strings/sets, asset ordering,
privacy monotonicity, duplicate revisions, illegal transitions, CAS conflict
and no raw prompt/examples in tables/logs.

## Rollback и evidence

Disable unvalidated profiles without deleting historical snapshots. Storage
migration remains additive/backup-protected; document the canonical identity
only after tests prove replayability.
