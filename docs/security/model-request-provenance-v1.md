# Model request provenance v1

EvoHime хранит model-visible provenance в Core-owned SQLite. Канонический
контракт находится в `contracts/model-request/v1/`, а Rust implementation — в
`crates/evohime-model-provenance`.

Инвариант: `MODEL_VISIBLE_MEANS_RECONSTRUCTABLE`. До provider dispatch envelope
проходит bounded validation и durable commit. Canonical bytes используют JCS и
domain-separated SHA-256 `evohime-model-request-v1\\0`; retry/fallback получают
новый `request_id`, сохраняя `logical_request_id`, `ledger_id` и lineage.

Секреты, API keys, Authorization, DPAPI plaintext и tool callbacks не являются
частью envelope/export. Sources, responses, tool intents, shadow originals,
tombstones и lifecycle states (`redacted`, `retention_pruned`) разделены и
проверяются typed-кодами. Offline bundle verifier не читает Core, SQLite,
renderer, workspace, сеть или provider settings.

Новые dispatchable записи используют `payload_mode=full`; legacy `hash_only`
строки остаются явно неполными и не могут быть отправлены provider.
