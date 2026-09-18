# План 180.1 — Profile/card contract и storage

## Изменить

1. Extend `ExternalAgentProtocolKind` with versioned A2A backend and define
   bounded `RemoteAgentProfile`, endpoint policy, auth-profile ref, skill/
   capability allowlists, privacy ceiling and concurrency limit.
2. Define untrusted `RemoteAgentCardSnapshot` with raw/normalized hashes,
   origin/expiry/protocol version, bounded skills/schema hashes and streaming/
   auth claims. Card claims never grant local capabilities.
3. Define delegation request, context capsule and task binding contracts with
   parent/request/capsule/capability/policy hashes, idempotency and bounded
   objective/artifact budgets.
4. Persist profile revisions, card snapshots, remote task bindings/IDs, states,
   hashes and recovery disposition through existing external-agent/local
   storage; never persist full cards, headers, cookies, transcripts or tokens.
5. Add deterministic state transition and revision/CAS rules including
   `UnknownOutcome`, input-required and auth-required.

## Зависимости

### Блокирующие

- `external_coding_agent_adapter.rs` and `external_coding_agent_adapter_store.rs`;
  existing bounded storage/migration conventions.

### Опциональные

- Protocol-specific wire schema may be added inside adapter boundary after the
  Core contract is stable.

## Проверка

Contract tests cover URL/profile validation, card hashing/expiry, unsupported
protocol, capability intersection, state transitions, revision conflict and no
secret/full-card persistence.

## Rollback и evidence

Disable a profile revision without removing task history. Keep old card
snapshots for replay/reconciliation only under stored endpoint policy; no new
task may use a disabled profile.
