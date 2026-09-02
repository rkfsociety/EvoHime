# План 125.3 — IPC, projection и UI

Статус: этап 3 для [плана 125.0](./125-0-free-provider-reliability-routing.md); после 125.2.

## Зависимости

### Блокирующие

- План 125.2, authenticated desktop IPC, replay/resync, generated TypeScript, main/preload bridge и существующие Provider/ModelPicker surfaces.

### Опциональные

- Diagnostics/evidence views и benchmark projection; при отсутствии показывать `Unknown` и причину.

## Реализация

1. Добавить additive IPC commands/events после проверки текущего highest tag: profile/catalog refresh, free/quota state, reliability snapshot, probe policy/status, route explanation, attempt chain и bounded policy actions.
2. Core валидирует provider/model identity, scope, catalog revision/hash, policy, quota, probe mode, access mode и requested selector. Renderer не может подать `Free`, `Healthy`, `Ready`, quota или provenance как authoritative.
3. Проецировать metadata-only поля: provider/model/upstream, free state/source/last checked, capability summary, reliability class/sample window/p95, quota/cooldown, region/access mode and explanation. Не передавать keys, raw headers/bodies, prompts, full logs or provider payloads.
4. В ModelPicker/Provider UI показать free/paid/unknown distinction, health with sample/window, p95/cooldown/quota, Local vs Cloud identity and “Why selected?”. Experimental/anonymous/shared modes должны быть явно помечены.
5. Сохранить replay/resync, correlation/idempotency, redacted errors and accessible keyboard/screen-reader states; stale/failure/unknown/paid are not collapsed into green state.
6. Добавить tests for forged projections/actions, stale free state, paid fail-closed, route explanation, attempt provenance, replay, bounds, redaction and accessibility.

## Критерии выхода

- [ ] UI is projection-only and displays why a route was selected.
- [ ] Free state, reliability, quota and cooldown remain visibly distinct.
- [ ] IPC replay, bounds, redaction and forged-authority tests pass.

## Не входит

Direct provider requests from renderer, local SQLite access, client-side scoring/evaluation, secret management and automatic policy relaxation.
