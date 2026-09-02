# План 121.3 — Local Model Performance Calibration: IPC, client projection и UI

Статус: этап 3 для [плана 121.0](./121-0-local-model-performance-calibration.md); после [плана 121.2](./121-2-local-model-performance-calibration.md).

## Цель

Дать Electron bounded Model Performance surface для calibration, progress, measured profiles, context curve, runtime comparison, stale diagnostics и retest без переноса measurement authority в renderer.

## Зависимости

### Блокирующие

- План 121.2 — runner commands/events, recovery and stable projections.
- Authenticated desktop IPC, sequence replay/resync, generated TypeScript protocol, main/preload bridge и existing Models/Benchmark navigation.

### Опциональные

- Plan 36 linkage, Diagnostics Bundle and optional CLI using same Core commands.

## Реализация

1. После проверки highest tag зарезервировать additive commands/events/results для list profiles, get samples/curve, start quick/extended calibration, cancel/retest, compare runtimes, reset bounded measurements and status. Preserve major, frame limits, correlation, idempotency and replay.
2. Core accepts only target identity/config refs and expected versions; revalidates verified runtime, safe context, resource policy, actor, budget and calibration mode. Renderer cannot submit TPS/TTFT as measured evidence or mark profile current.
3. Передавать metadata-only projections: exact model/runtime/hardware/config identity hashes, session state/progress, measured/unknown/stale metrics, median/percentiles/variance/failure rate, context points/headroom/confidence, contention and typed failure.
4. Связать `ipc_bridge.rs`, shared API, preload/main adapters and reconnect/replay; partial/cancelled/unknown state visibly differs from Completed/MeasuredLocal.
5. Add Settings → Models → Local → Performance: Fit estimate versus Measured locally/Measured stale/Not measured, TPS/TTFT/memory, context `comfortable/interactive/maximum measured`, stability, runtime/date, calibration actions and warnings.
6. Add runtime comparison view with per-purpose tradeoffs, sample detail bounded to metadata and explicit `Retest`; avoid universal winner or quality score language.
7. Show recommendation/routing impact as a Core-provided signal and distinguish MeasurementReady, RecommendationChanged and ActiveSelectionChanged. Never silently replace explicit model choice.
8. Optional CLI delegates to same Core protocol; reset removes eligible local measurement history only through Core retention policy and does not affect installed model/runtime.

## Acceptance-to-projection matrix

- `C01` calibration → target/config/session/progress and typed outcome.
- `C02` profile → measured/unknown/stale metrics, stability, confidence and provenance hash.
- `C03` context curve → bounded points/headroom/derived classes within safe ceiling.
- `C04` comparison → per-runtime metrics/tradeoffs, no universal quality winner.
- `C05` routing → Core signal and explicit-selection boundary.
- `C06` privacy/security → no raw prompts, hardware identifiers, credentials or forged results.

## Критерии выхода

- [ ] IPC additive, authenticated, bounded and replay-safe.
- [ ] Mutations are Core-validated/idempotent and return typed busy/stale/denied outcomes.
- [ ] UI remains projection-only and represents unknown/stale/partial honestly.
- [ ] Sensitive data and raw benchmark payloads are absent from renderer state.

## Не входит

Direct runtime/filesystem access, client-side aggregation/routing, public sharing, arbitrary tuning controls и second benchmark UI authority.
