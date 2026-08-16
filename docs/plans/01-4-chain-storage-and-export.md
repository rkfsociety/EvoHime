# Этап 01.4: Chain storage и export

Этап плана [01 Подписанные hash-chain receipts](01-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующие: 01.1 (canonical payload/envelope v1), 01.2 (public-key history,
trust roots и offline verifier) и 01.3 (receipt/action rows, chain append,
approval binding и recovery state).

Это последний этап плана 01. Он не меняет signed bytes и не создаёт новый
runtime path; он делает durable storage, verification, read-only IPC, export и
UI projection поверх уже записанных receipts.

## Источник истины и границы

Core-owned SQLite events.db — единственный mutable source of truth для
receipt_records, action state, chain heads и signed checkpoints. JSONL export —
immutable snapshot, производный от одной SQLite read transaction; live dual-write
в SQLite и JSONL запрещён.

Renderer не читает SQLite/filesystem и не вычисляет подписи. Electron main
вызывает только authenticated read-only IPC. Offline verifier читает export
bundle и public key history без Core, сети и private key.

Export изменяет только внешний destination directory, не Core state. Destination
должен быть выбран shell/user и проверен как canonical path; overwrite
существующего bundle запрещён без отдельного user-confirmed replace operation.

## Схема SQLite receipts_v1

Миграция receipts_v1 выполняется транзакционно существующим LocalDatabase
migration runner с backup до изменения схемы. Foreign keys и WAL включены.
Существующие audit rows не переписываются и не объявляются signed receipts.

### receipt_records

| Column | Type / constraint |
| --- | --- |
| sequence | INTEGER PRIMARY KEY AUTOINCREMENT; commit order, not signed payload |
| receipt_id | TEXT NOT NULL UNIQUE; lowercase UUIDv7 |
| action_id | TEXT NOT NULL; lowercase UUIDv7 |
| receipt_kind | TEXT NOT NULL CHECK pre_action/post_action/refusal |
| action_status | TEXT NOT NULL CHECK prepared/succeeded/failed/cancelled/refused |
| task_id, run_id, tool_name | TEXT NOT NULL; bounded values from 01.1 |
| key_id | TEXT NOT NULL; ed25519:<64 lowercase hex> |
| receipt_hash | TEXT NOT NULL UNIQUE; 64 lowercase hex |
| previous_receipt_hash | TEXT NULL only for genesis chain row |
| canonical_payload | BLOB NOT NULL; 1–4096 bytes |
| canonical_envelope | BLOB NOT NULL; 1–8192 bytes |
| created_at_ms | INTEGER NOT NULL; derived from canonical timestamp |
| source | TEXT NOT NULL CHECK signed; legacy audit is exposed by a separate adapter |

canonical_payload and canonical_envelope are exact bytes used for signature
and hash verification, not a reserialization cache. Database NULL is allowed
only for the genesis predecessor and internal terminal hashes; JSON/JSONL never
serializes those as null.

Indexes:

- UNIQUE receipt_id;
- UNIQUE receipt_hash;
- (task_id, created_at_ms, sequence);
- (run_id, created_at_ms, sequence);
- (action_id, sequence);
- (key_id, sequence).

### receipt_actions

| Column | Type / constraint |
| --- | --- |
| action_id | TEXT PRIMARY KEY |
| pre_receipt_hash | TEXT NULL until pre append, then FK receipt_records |
| terminal_receipt_hash | TEXT NULL until post/refusal |
| state | prepared/terminal/pending_recovery |
| approval_id | TEXT NULL; bounded typed identifier |
| approval_call_hash | TEXT NULL; 64 lowercase hex |
| approval_state | none/pending/granted/denied/expired/claimed |
| tool_args_hash | TEXT NOT NULL; 64 lowercase hex |
| recovery_code | TEXT NULL; stable enum, no free error text |
| created_at_ms, updated_at_ms | INTEGER NOT NULL |

The row is updated only in the same transaction as its receipt append. A
terminal row must have exactly one terminal receipt; a pending row must have
pre_receipt_hash and no terminal hash. approval_call_hash makes offline
approval-binding verification possible even after in-memory PermissionEngine
state is gone.

### receipt_chain_heads

key_id TEXT PRIMARY KEY, head_sequence INTEGER NOT NULL,
head_receipt_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL.

One BEGIN IMMEDIATE append reads this row, verifies the predecessor, inserts
receipt_records and updates the head. A stale head causes bounded retry and then
receipts.chain_conflict; the tool is not run after such a failure.

### receipt_checkpoints

Used only when retention compacts a prefix:

checkpoint_id, key_id, cutoff_sequence, first_retained_hash,
prefix_last_hash, last_deleted_receipt_hash, created_at, canonical_checkpoint,
signature, status.

The checkpoint object is JCS ReceiptCheckpointV1, signed by the key active at
checkpoint creation. It proves the retained suffix begins after the stated
prefix hash; it does not pretend deleted receipts were individually verified.

## Metadata, privacy и retention

Bounded receipt metadata is exactly the columns above plus the typed payload
fields from 01.1. Raw tool arguments, result, prompt, response, stdout/stderr,
paths beyond typed scope, provider response, error text and private material are
never stored in receipt_records, exports or diagnostics. Only hashes, enums,
identifiers and bounded canonical bytes are retained.

Retention v1:

- terminal receipts: 90 calendar days and maximum 100,000 retained rows per
  key, whichever is reached first;
- pending_recovery and action rows with no terminal: retained until explicit
  reconciliation, never automatic deletion;
- before deleting a prefix, create and fsync a signed ReceiptCheckpointV1,
  export it, then delete only rows strictly before cutoff in one transaction;
- a chain with checkpoint reports verified_pruned, not full verified;
- deletion is never performed during export or verification and never removes a
  row selected by an active export snapshot;
- explicit user purge is a separate approved operation and leaves checkpoint
  metadata plus an audit event.

Legacy unsigned audit records remain available through a separate adapter as
source=legacy until existing audit retention removes them; they are always
unverified and never inserted into receipt_records.

## JSONL export bundle v1

ExportReceipts creates a directory bundle atomically:

    <destination>\
      manifest.json
      receipts.jsonl
      actions.jsonl          (present when selected range has action state)
      key-history.jsonl
      checkpoints.jsonl       (present only when snapshot includes checkpoints)
      trusted-roots.json      (present only when explicitly requested)

All files are UTF-8 without BOM. JSONL uses one JCS object per line, LF
terminators, no header, no blank lines and a final LF. A line is ≤12,288 bytes;
the embedded receipt envelope remains limited to 8,192 bytes.

### receipts.jsonl record

Each line is canonical JCS:

    {
      "record_version": 1,
      "record_kind": "receipt",
      "sequence": "123",
      "receipt_hash": "<64 lowercase hex>",
      "canonical_envelope": "<unpadded base64url>"
    }

sequence is a decimal string so JSON number precision cannot drift. The
base64url decodes to the exact canonical envelope bytes from SQLite; the
verifier recomputes receipt_hash and never trusts duplicated display metadata.
The embedded payload supplies action_id, previous_receipt_hash, approval_id and
all other signed fields.

key-history.jsonl contains exact canonical KeyTransitionV1 records from 01.2,
one per line. checkpoints.jsonl contains exact signed ReceiptCheckpointV1
records. trusted-roots.json is optional, user-selected trust metadata and is
never silently treated as a trust root on another machine.

`actions.jsonl` содержит bounded projection action rows, включая
`pending_recovery`, `recovery_code` и `requires_reconciliation`; raw input,
result и error text запрещены. Эта projection signed receipt не является и
проверяется только на согласованность с `receipt_actions`. Отсутствие
`actions.jsonl` в архиве без action projection не является ошибкой.

### manifest.json

Manifest is canonical JSON:

    {
      "manifest_version": 1,
      "export_id": "<uuidv7>",
      "created_at": "<canonical timestamp>",
      "snapshot_last_sequence": "123",
      "selected_count": 12,
      "record_count": 42,
      "first_receipt_hash": "<sha256 hex>",
      "last_receipt_hash": "<sha256 hex>",
      "files": [
        {"name":"receipts.jsonl","bytes":1234,"sha256":"<64 hex>"},
         {"name":"key-history.jsonl","bytes":456,"sha256":"<64 hex>"},
         {"name":"actions.jsonl","bytes":789,"sha256":"<64 hex>"}
      ]
    }

Optional values are omitted, never null. Manifest file hashes detect export
corruption, while receipt/hash-chain signatures determine cryptographic trust.

Export algorithm:

1. authorize and canonicalize destination; reject existing destination;
2. begin SQLite read transaction and capture snapshot sequence;
3. stream bounded rows into staging directory without loading full export into
   memory;
4. copy the complete public-key path and checkpoint boundary required by the
   selected receipt range; omitting a trust transition makes the export
   unverified rather than silently shortening the chain;
5. fsync files and staging directory, write manifest, fsync again;
6. atomic rename staging directory to destination;
7. record bounded receipts.exported audit event with export id, snapshot range,
   count, file hashes and outcome.

Failure removes only the uniquely named staging directory, never source rows.
Crash leaves no directory with a valid manifest until the final rename; the next
startup scans the export parent for uniquely named staging directories and
removes them automatically after validating that they are owned by EvoHime and
contain no committed destination marker.

## Verify-chain algorithm

Verification operates on a consistent SQLite snapshot or an export bundle:

1. validate input size, manifest file hashes and JSON/JCS bytes; timestamp
   проверять синтаксически всегда, а skew — только при явно включённом
   offline policy flag;
2. load public key history and explicit trust roots;
3. for each selected receipt in sequence order, check envelope size,
   canonical bytes, receipt hash and Ed25519 signature over canonical payload;
4. resolve key_id through signed transition history and verify trust path from
   pinned genesis;
5. compare previous_receipt_hash to the preceding selected row or a trusted
   checkpoint boundary; detect deletion, reorder, duplicate, fork and cycle;
6. verify action_id pairing: pre exists before terminal, tool_args_hash is
   unchanged, approval_id/approval_call_hash match, and terminal status agrees
   with receipt_kind;
7. classify each row and the chain; never repair input or return partial
   success.

Default verification is full for the selected range. An incremental cache may
skip an unchanged prefix only after rechecking its stored head hash, public
history hash and checkpoint; cache is an optimization, never a trust source.

Фильтры task/run/action не могут скрыть predecessor: VerifyReceipts расширяет
выбранный диапазон до chain closure между первой и последней sequence (либо до
доверенного checkpoint). ExportReceipts делает то же самое и включает в bundle
контекстные chain rows; selected_count и record_count в manifest различаются.
Если closure превышает limit, операция завершается receipts.limit_exceeded, а
не выдаёт частично verified chain.

Approval binding status is binding_verified when receipt_actions contains the
same approval id and call hash. It is approval_granted only when the durable
approval audit row records the same id, call hash and granted decision. A
missing approval audit is unverified, not success.

Key status rules:

- retired key is valid for receipts whose timestamp precedes its rotation;
- compromised transition makes receipts at/after its transition stale_key, even
  if the Ed25519 signature is mathematically valid;
- missing public history/genesis pin is unverified;
- unknown key id is unverified/key_unknown, not broken;
- invalid signature, receipt hash, predecessor or canonical bytes is broken.

Statuses:

- verified — complete trusted chain, signatures, action/approval binding; no
  receipt before the selected range was pruned;
- verified_pruned — trusted signed checkpoint plus valid retained suffix;
- pending — pre exists without terminal receipt, no success claim;
- stale_key — signature valid but key trust is invalid after compromise;
- unverified — legacy/unknown key/missing trust or missing approval evidence;
- broken — tamper, digest mismatch, invalid signature, wrong predecessor,
  fork, cycle or malformed record.

Stable verify codes:

receipts.invalid_json, receipts.non_canonical, receipts.manifest_mismatch,
receipts.hash_mismatch, receipts.signature_invalid,
receipts.chain_incomplete,
receipts.previous_mismatch, receipts.digest_mismatch,
receipts.approval_unverified, receipts.missing_receipt, receipts.key_unknown,
receipts.stale_key, receipts.chain_fork, receipts.chain_cycle,
receipts.pending, receipts.empty_range, receipts.unsupported_version,
receipts.db_unavailable, receipts.export_io.

## IPC contract

All methods use authenticated desktop-ipc-v1 named pipe. Allowed roles:
shell and compatibility-shell; both are read-only for list/verify. Core
rechecks role, session, bounds and task scope. Renderer cannot call Core.

### ListReceipts

Request:

- optional task_id/run_id/action_id;
- optional from/to UTC RFC3339 timestamps, inclusive start/exclusive end;
- optional status filter;
- limit 1–500 (default 100);
- opaque cursor, max 256 bytes.

Response:

- snapshot_last_sequence as decimal string;
- bounded ReceiptSummary rows (receipt id, action id, kind/status, task/run,
  key id, timestamp, receipt hash, previous hash, verification status);
- next_cursor omitted at end.

No canonical payload or raw content is returned automatically.

### VerifyReceipts

Request:

- same filters as ListReceipts;
- limit 1–2,000 (default 500);
- optional trust key id;
- include_pending boolean.

Response:

- overall status and stable code;
- checked count, snapshot sequence;
- per-receipt bounded status/code/hash/key id;
- chain_start_hash and chain_end_hash.

It is synchronous only within the bounded limit. Larger verification is
performed by offline verifier/export, not an unbounded pipe request.

### ExportReceipts

Request:

- destination directory selected by shell;
- same task/run/action/date filters;
- limit 1–100,000;
- include_public_history boolean (default true);
- include_trusted_roots boolean (default false);
- replace=false (must remain false in v1).

Response contains export_id, destination basename, snapshot sequence, count,
manifest SHA-256 and outcome. Absolute arbitrary destination paths, network
shares, private-key files and overwrite are rejected.

IPC errors are structured and bounded:
receipts.access_denied, receipts.invalid_filter,
receipts.limit_exceeded, receipts.not_found, receipts.db_unavailable,
receipts.export_exists, receipts.export_io, receipts.verify_failed.
Full paths, payloads and secret-like values never enter error text or audit.

## Compatibility and migration

Migration creates receipts_v1 tables and indexes with no destructive rewrite.
During the transition:

- new runtime receipts write only the new signed tables plus the existing
  audit event sink;
- legacy audit events remain queryable through the source=legacy adapter with
  status unverified;
- old clients receive the previous protocol major/version error rather than
  guessing receipt fields;
- generated TypeScript and compatibility C# IPC types are updated together;
- JSONL export is versioned by manifest_version and record_version; unknown
  versions fail closed with unsupported-version code.

There is no SQLite/JSONL dual-write consistency problem: SQLite commits first and
export is a snapshot after commit. SQLite is authoritative if an existing export
differs: the export is stale/corrupt, is never imported back into SQLite, and a
new export must be generated from a fresh snapshot. The consistency test compares
every exported receipt/action hash and manifest file hash with the source
snapshot, including process termination during staging.

## Audit, UI и access

Every list, verify, export, purge and failed operation writes bounded audit
events (receipts.listed, receipts.verified, receipts.exported,
receipts.purge, receipts.failed) with caller role, filter digest, snapshot
sequence, count, status and error code. No raw filter values or payloads are
logged.

UI mapping:

- verified → green “verified” only for complete trusted chain;
- verified_pruned → “verified, history compacted” with checkpoint boundary;
- pending → “pending recovery”, never success;
- stale_key → “stale/compromised key”;
- unverified → “not trusted/legacy evidence”;
- broken → “chain broken” with first stable code and sequence.

UI shows key id, receipt/action hashes, timestamp and bounded status. It never
renders canonical payload automatically and never labels a receipt as correct.

## Артефакты этапа

- migration SQL: `crates/evohime-local-storage/migrations/*_receipts_v1.sql`;
- `contracts/receipts/v1/export-manifest.schema.json`;
- `contracts/receipts/v1/export-record.schema.json`;
- `contracts/receipts/v1/action-projection.schema.json`;
- `contracts/receipts/v1/checkpoint.schema.json`;
- generated IPC additions in
  `crates/desktop-ipc/proto/evohime.desktop.proto`;
- shared JSONL/verification fixtures in
  `contracts/receipts/v1/export-vectors.json`;
- offline verifier consumes the same schemas and vectors as Core/Electron.

## Проверки

- migration/DDL tests: constraints, indexes, foreign keys, backup and restart;
- JSONL vectors: LF/UTF-8, final newline, no nulls, line/manifest/file hashes,
  base64 exact bytes, pending_recovery action markers and schema rejection;
- full verify vectors for valid chain, empty range, genesis, rotation,
  checkpoint, broken predecessor, deletion/reorder, duplicate/fork/cycle,
  invalid signature, stale/unknown key, digest mismatch, missing receipt and
  pending action;
- unsupported-version vector: `receipt_version>1` returns
  `receipts.unsupported_version` and does not mark the chain broken;
- approval binding tests distinguish binding_verified, approval_granted and
  approval_unverified;
- SQLite snapshot/export consistency under process termination and disk-full
  staging failure;
- IPC role, bounds, cursor, date range, access-denied and DB-error tests;
- performance regression: verify 1,000 receipts ≤2 seconds p95 and export
  10,000 receipts ≤5 seconds p95 on reference CI runner, with bounded streaming
  memory ≤64 MiB;
- legacy audit remains visible as unverified and never becomes signed;
- UI status tests cover every status/code mapping and never display raw payload.

## Критерии готовности

- DDL, indexes, constraints and migration are implemented and tested;
- SQLite is documented and tested as source of truth; export is an atomic,
  snapshot-consistent derived bundle;
- JSONL/manifest/key-history/checkpoint formats and exact byte rules are
  documented, schema-validated and consumed by offline verifier;
- verify-chain checks canonical bytes, receipt hash, signature, predecessor,
  key trust, action pairing and approval binding with stable error codes;
- retention/privacy rules are explicit, pending actions are protected, and
  compaction uses signed checkpoints with verified_pruned status;
- ListReceipts, VerifyReceipts and ExportReceipts IPC signatures, bounds,
  roles, access control and errors are covered by compatibility tests;
- SQLite/export crash consistency, deletion/reordering/tamper and all six
  statuses are tested;
- performance budgets pass and no raw arguments/results/secrets/private keys
  appear in DB, export, IPC, audit or UI.
