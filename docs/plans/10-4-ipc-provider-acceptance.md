# 10-4 — Acceptance и Electron/Core contract closure

## Цель

Подтвердить end-to-end границы IPC, negotiation, provider/worker adapters и
target lifecycle на deterministic fixtures без credentials и внешнего
provider network.

## Зависимости

### Блокирующие

- 10-1, 10-2 и 10-3 с их Rust/Electron fixtures;
- принятые Rust и Electron contract suites текущего desktop IPC;
- generated proto types синхронизированы между Rust и Electron.

### Опциональные

- real-Core E2E с локальным mock provider. При отсутствии собранного Core
  тест помечается skip по существующему правилу и не подменяется зелёным
  результатом fake acceptance;
- packaging smoke: он нужен для release audit, но не заменяет contract tests.

Реальный сетевой provider в матрицу не входит: все provider-сценарии идут
через fake/mock adapter без credentials.

## Deterministic acceptance matrix

### Negotiation и session

- compatible `CoreInfo`, auth challenge, handshake и Ready;
- same-major minor downgrade и capability intersection;
- major mismatch, missing required feature, malformed/zero/oversized limits;
- unavailable Core, auth rejection, reconnect и stale session;
- instance/epoch change очищает queue/cache и запускает bounded replay;
- oversized frame/request отвергается до изменения projection.

### Provider/worker boundary

- provider unavailable: same-target fallback либо typed terminal failure без
  dispatch в старый route;
- unsupported descriptor version и capability mismatch до эффекта;
- fake worker получает только grant subset, bounded scope и opaque secret ref;
- path, SQLite handle, raw secret, prompt и capability overgrant отвергаются;
- timeout/cancellation дают один terminal typed result без blind retry;
- provider/worker diagnostics redacted и bounded.

### Target/projection

- workspace/route/backend switch во время queued и running operation;
- stale response после switch не попадает в projection и не порождает side
  effect;
- уже начатый внешний эффект становится `unknown_outcome`, не повторяется;
- provider credential change проходит через новый Core generation;
- replay после restart принимает только current epoch/target metadata;
- два workspace target не пересекаются по path, query и secret scope.

### Shell boundary

- renderer API работает только через preload/contextBridge;
- static/security test запрещает renderer imports для pipe, HTTP, protobuf,
  SQLite и filesystem;
- `provider.*` сохраняет только shell summary и не раскрывает ключ;
- legacy `Ready` без `core_info` остаётся совместимым для старого Electron
  consumer;
- `ProviderSummary` в renderer не содержит ключ, а `restarted` соответствует
  фактической смене `core_instance_id/session_epoch`.

## Проверки и команды

- Rust: `cargo test --locked -p evohime-desktop-ipc -p evohime-core -p evohime-model-gateway`
  и targeted adapter/target tests;
- Electron из `desktop/evohime-electron`: `npm run check:protocol`, `npm run typecheck`,
  `npm test`;
- Electron contract suite и targeted Rust desktop-ipc tests;
- static-проверки на запрещённые renderer imports, прямые transport calls и
  redaction;
- `git diff --check`, а когда в объёме release packaging — ещё и
  `pwsh -File scripts/native-package.tests.ps1`.

Каждый acceptance test должен указывать fixture, expected typed code,
dispatch count и projection result. Простого `ok` или HTTP status без
проверки фактического state transition недостаточно.

## Закрытие

После прохождения матрицы контракт переносится в `docs/architecture.md`,
подтверждённое состояние и конкретные тесты — в `docs/current-state.md`,
обновляются `docs/development-plan.md` и `docs/plans/README.md`, generated
protocol references сверяются, затем удаляются все файлы плана 10. Удаление
допустимо только task-only коммитом после свежих проверок;
неполная реализация остаётся в этих планах с явным разделом о реализованном
и недостающем поведении и не помечается выполненной.
