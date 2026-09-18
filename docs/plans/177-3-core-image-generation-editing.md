# План 177.3 — Authenticated IPC и projection

## Изменить

1. Добавить additive protobuf command/event surface for capability inspection,
   explicit image request, status, cancel and artifact metadata read through
   existing authenticated bridge and sequence/replay limits.
2. Generate/check matching Electron types and main/preload adapters. Renderer
   receives state, hashes, dimensions, MIME, artifact refs and bounded refusal/
   failure reason only; raw binary is served through existing artifact owner.
3. Integrate with existing operations/workbench surface rather than creating a
   mandatory top-level image tab. UI cannot choose provider, alter grants,
   bypass policy or write workspace files.
4. Expose typed unavailable/unknown/stale/approval/unknown-outcome states and
   preserve idempotency on reconnect/replay.

## Почему именно там

The repository’s IPC is the compatibility boundary. Keeping projections
metadata-only preserves Core ownership and avoids copying binary/image state
into Electron or event history.

## Зависимости

### Блокирующие

- Plan 177.1–177.2 and existing proto generation/check protocol gate.
- Existing authenticated IPC role/session and ArtifactStore read boundary.

### Опциональные

- Existing workbench/operations panel can opt in after additive API exists.

## Проверка

- Rust IPC replay/sequence/frame-size tests and generated TypeScript protocol
  check.
- Electron adapter/type tests for bounded projections, reconnect and unknown
  states; assert no prompt/image bytes or credentials reach renderer.
- Manual/static review confirms no new renderer-side provider or workspace path.

## Rollback и evidence

Keep new protobuf fields additive and feature-gated. If UI support is disabled,
Core/CLI can still expose typed unavailable while historical metadata remains
readable; document compatibility and release checks in canonical docs.
