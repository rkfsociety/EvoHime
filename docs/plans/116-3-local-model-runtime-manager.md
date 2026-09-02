# План 116.3 — Local Model Runtime Manager: IPC, client projection и UI

Статус: этап 3 для [плана 116.0](./116-0-local-model-runtime-manager.md); после [плана 116.2](./116-2-local-model-runtime-manager.md).

## Цель

Дать Electron bounded projection и явные user actions для hardware, recommendations,
installed models, progress, health, activation и recovery без переноса authority из Core.

## Зависимости

### Блокирующие

- План 116.2 — runtime commands/events, recovery и stable result types.
- Authenticated desktop IPC, sequence replay/resync, generated TypeScript protocol,
  Electron main/preload adapter и existing Settings navigation.

### Опциональные

- Plan 53 diagnostics и Plan 105 context/cache presentation; без них UI показывает
  базовые bounded metadata projections.

## Реализация

1. Зарезервировать additive proto names/tags после проверки текущего highest tag;
   сохранить major, frame limits, correlation/idempotency/version и replay semantics.
2. Добавить commands/results/events для rescan hardware, list fits/catalog,
   install/cancel/update/remove, start/stop/probe, set preferred/bootstrap policy и
   refresh status. Core принимает identity refs, но не доверяет renderer hashes,
   paths, trust или readiness.
3. Связать `ipc_bridge.rs`, shared API, preload и main shell bridge. Adapter только
   сериализует command и передаёт event; credentials, artifact bytes, prompts,
   outputs, executable path и hidden reasoning не выходят в renderer.
4. Добавить Settings → Models → Local: Hardware, Recommendations, Installed,
   progress/health, active context, failure reason, Make preferred и explicit
   rescan/cancel/remove actions. `Fast/Balanced/Quality/LargestCompatible` — только
   Core-provided presentation.
5. Проверить reconnect/replay gap, duplicate events, stale action, unavailable
   runtime и optimistic conflict. UI не вычисляет fit/lifecycle и не запускает binary.

## Acceptance-to-projection matrix

- `C01` Hardware → bounded CPU/GPU/RAM/VRAM/runtime/disk projection и refresh.
- `C02` Recommendations → exact display name/revision/quantization/size/context,
  status/reasons/capability badges.
- `C03` Installed → verification/runtime/health/session/context/disk metadata и actions.
- `C04` Progress → Core state `Downloading/Verifying/Loading/Health check/Ready/Failed`.
- `C05` Bootstrap/switch → visible activation policy and snapshot boundary, без hidden switch.
- `C06` Security → IPC cannot forge trusted state or bypass Core policy.

## Критерии выхода

- [ ] Surface authenticated, additive и bounded.
- [ ] Every mutation is Core-checked, idempotent/versioned and has typed denial/stale outcome.
- [ ] UI reflects Core state after replay/reconnect and never owns lifecycle logic.
- [ ] Sensitive payloads, credentials, raw prompts/outputs and executable paths absent.

## Не входит

Direct filesystem/SQLite access, direct HTTP endpoint access, renderer hardware
probing, arbitrary executable picker и second model-management UI stack.

