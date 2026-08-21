# 10-2 — Adapter boundary и provider settings

## Цель

Сделать каждый Core-owned provider/worker вызов проходящим через проверяемый
adapter, не перенося policy, secrets или workspace ownership в renderer.

## Что уже есть в checkout

- `crates/model-gateway/src/provider_contract.rs` уже задаёт
  `CapabilityMetadata`, health snapshot, route policy snapshot и health
  overlay;
- routing выбирается Core, а provider credentials собираются supervisor из
  зашифрованного Electron main state;
- `provider.get/save/clearKey` обслуживаются Electron main и используют
  `provider.json`; renderer видит только summary;
- Core workflow adapters уже существуют, но это orchestration adapters, а не
  новый generic provider transport.

## Зависимости

### Блокирующие

- 10-1 для negotiated protocol/capability/limit context;
- контракты 09-1…09-4 после их принятия для immutable capability snapshot,
  policy, approval, redaction и cancellation;
- существующий model-gateway provider contract и supervisor secret boundary.

### Опциональные

- 07-2 manifest catalog. При его отсутствии descriptor строится из
  зарегистрированного route/provider contract;
- внешние worker runtimes. До их появления тестируется только fake worker
  adapter и builtin Core path.

## Контракт границ

1. Зафиксировать две разные adapter surfaces:

   - `renderer → preload/contextBridge → Electron main`: typed shell API;
   - `Electron main → Core`: единственный `CorePipeClient`/main adapter,
     который владеет framing, auth, negotiation, queue, reconnect и resync.

   Renderer не импортирует generated protobuf, `node:net`, HTTP client,
   filesystem/SQLite API и не получает pipe/secret.

2. Для Core adapters определить bounded internal contract
   `adapter/v1`:

   - descriptor: stable adapter id, contract version, capability metadata,
     limits and health;
   - session: negotiated capabilities, immutable policy/capability snapshot,
     target generation, deadline/cancellation and scope grants;
   - request/result: correlation id, bounded input/output, typed status and
     redacted diagnostic.

   Descriptor должен переиспользовать `CapabilityMetadata`,
   `CandidateHealthSnapshot`, `RoutePolicySnapshot` и `RunHealthOverlay`.
   Отдельный `ProviderAdapterInfo` не создаёт параллельные capability,
   health или limit values.

3. Разделить model provider и worker input:

   - model provider получает только уже policy-filtered model request, route
     snapshot, deadline/cancellation и bounded context; workspace path,
     SQLite handle и raw secret ему не выдаются;
   - worker получает capability-scoped session, opaque workspace scope,
     allowed path grants и opaque `SecretRef` только если конкретный worker
     требует его по контракту;
   - secret value не сериализуется в IPC, descriptor, event, receipt или
     worker request. Текущий env-based supervisor delivery сохраняется как
     implementation detail до отдельного secret-ref adapter.

4. Provider settings остаются отдельной shell-local поверхностью:

   - `provider.*` валидирует bounded provider/model/base URL, сохраняет ключ
     через safeStorage и возвращает summary;
   - изменение credentials перезапускает supervisor/Core и создаёт новую
     `core_instance_id/session_epoch`;
   - `SelectModelRequest` и route hints не меняют policy, workspace scope,
     approval или secret boundary.

5. Перед каждым dispatch Core валидирует descriptor version, negotiated
   capability, policy hash, target generation, scope и limits. Typed failures:
   `adapter_unavailable`, `adapter_unsupported`, `capability_mismatch`,
   `scope_denied`, `timeout`, `cancelled`, `stale_session`.

6. Fallback разрешён только среди routes из immutable snapshot текущего run и
   с тем же active target. После target/session change старый fallback не
   может выбрать другой workspace, provider config или backend.

## Проверки

- provider unavailable даёт deterministic same-target fallback или typed
  terminal failure, без dispatch в отключённый route;
- unsupported adapter version и capability mismatch отклоняются до эффекта;
- fake worker не может получить path вне grant, SQLite handle, raw secret или
  capability сверх snapshot;
- provider request не содержит workspace path/secret, а redacted result не
  содержит raw provider error или prompt;
- renderer static/security test не находит direct pipe/HTTP/protobuf import;
- adapter calls имеют bounded timeout/cancellation и не оставляют новую
  попытку;
- descriptor/route snapshot round-trip и limits имеют deterministic fixtures.

## Готово, когда

Core остаётся единственным источником решения о доступности инструмента,
provider и workspace; shell-local provider settings не смешаны с provider
runtime contract; каждый adapter получает минимальный bounded session и не
может выйти за его policy, target или secret scope.
