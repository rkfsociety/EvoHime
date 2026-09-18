# План 180.2 — Network lifecycle, capsule и recovery

## Изменить

1. Implement profile-owned discovery with bounded response bytes/depth/text,
   scheme/host/port/userinfo validation, TLS checks, redirect-off default and
   per-hop policy when redirects are explicitly allowed.
2. Reuse the existing SSRF/network policy to block public-profile loopback,
   private, link-local and metadata targets, DNS rebinding and dynamic model/
   user-proposed origins.
3. Build immutable capsule from selected context/loadout refs, parent grant
   intersection and privacy policy; exclude credentials, paths, pipe secrets,
   hidden reasoning and unselected workspace.
4. Submit/status/cancel through bounded streaming or polling. Retry discovery
   only under safe policy; after possible submit, lost response is
   `UnknownOutcome`; known ID reconciles the same remote task.
5. Validate inline/binary remote results before ArtifactStore publication; do
   not auto-fetch remote URLs. Use existing background, rate and concurrency
   owners and recover all nonterminal states after restart.

## Зависимости

### Блокирующие

- Plan 180.1 and existing research/network SSRF, context/privacy, artifact and
  durable background owners.

### Опциональные

- SSE/streaming can be enabled only when the frozen card confirms it; polling
  remains a compatible fallback without public push listener.

## Проверка

Network tests simulate SSRF/private/redirect/rebind/userinfo/TLS failures,
oversized card, lost submit, known-ID reconcile and auth downgrade. Capsule
tests assert allowlist/grant/privacy/hash; artifact tests assert MIME/schema/
quota/hash and URL non-fetch. Recovery tests cover every nonterminal state.

## Rollback и evidence

Disable new dispatch while preserving profile/card/task metadata. Never issue a
new remote task as rollback after ambiguous submit; only explicit user retry
with new idempotency identity may do so.
