# План 116.0 — Local Model Runtime Manager: hardware-aware deployment, verified model catalog и safe bootstrap switchover

Статус: предложено по [issue #96](https://github.com/rkfsociety/EvoHime/issues/96). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Добавить Core-owned контур, который проводит локальную модель по цепочке:

```text
hardware discovery -> compatible recommendation -> verified artifact
-> supervised runtime -> health gate -> ModelProfile -> local inference
```

Пользователь должен получать пригодную локальную модель без ручного подбора
аппаратных ограничений, проверки файлов, запуска endpoint и регистрации профиля.
Опциональный bootstrap-профиль позволяет работать на небольшой модели до
готовности предпочтительной, без смены модели внутри уже начатого вызова.

## Текущее основание и граница

В checkout уже существуют `ModelProfile`/Model Gateway, Model Resilience Policy,
Execution Backend Registry и loopback local provider. Новый контур расширяет их,
но не создаёт второй gateway или agent runtime. Core остаётся authority для
hardware, catalog, trust, lifecycle, selection, context и provenance; Electron
остаётся projection-only. Supervisor/execution boundary владеет процессами.

MVP поддерживает один allowlisted supervised OpenAI-compatible loopback adapter
через абстрактный runtime registry. Backend binary должен быть bundled либо
доставлен отдельным проверяемым release-артефактом; model repository не поставляет
Python/Node/custom code и не получает права на workspace, tools или credentials.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./116-1-local-model-runtime-manager.md)
- [Этап 2 — runtime-интеграция и recovery](./116-2-local-model-runtime-manager.md)
- [Этап 3 — IPC, client projection и UI](./116-3-local-model-runtime-manager.md)
- [Этап 4 — verification, release-evidence и закрытие](./116-4-local-model-runtime-manager.md)

## Зависимости

### Блокирующие

- План 42 / Model Resilience Policy v1 из `docs/architecture.md`.
- План 43 / Execution Backend Registry v1 из `docs/architecture.md`.
- План 67 / Schema-Driven Agent Configuration для durable activation policy.
- План 115 / Model Purpose Routing для purpose-aware profile selection.
- Existing Core capability/policy/approval, SQLite migration/backup, event journal,
  context budget, authenticated IPC, supervisor Job Object и provenance boundaries.

### Опциональные

- План 36 / Agent Benchmark Matrix: измерения производительности; без него fit
  остаётся conservative estimate, а не benchmark.
- План 41 / Execution Policy Profiles: дополнительные runtime limits; без него
  применяются встроенные строгие limits.
- План 46 / Agent Role Profiles: role-specific model purposes; без него работает
  базовая purpose policy.
- План 53 / Diagnostics & Support Bundle: расширенный redacted export; без него
  остаются обычные metadata-only diagnostics.
- План 105 / Prompt Cache Planner: provider-aware cache hints; не блокирует MVP.

## Основной контракт

Core вводит versioned сущности `LocalHardwareProfile`, `LocalModelDescriptor`,
`LocalInferenceRuntime`, `LocalModelFit`, `LocalModelRuntimeSession` и
`LocalModelManagerPolicy`. Exact immutable revision/hash обязательны для catalog,
artifact, runtime и session. `Unknown` не повышается до `Compatible`, а
`Unsupported` не запускается автоматически.

Artifact lifecycle: `NotInstalled -> Queued -> Downloading -> Verifying ->
Installed -> Loading -> Probing -> Ready`, с typed `Failed`, `Updating` и
`Removing`. Только проверенный hash после disk preflight получает atomic promotion;
staging-файл никогда не считается установленным.

Fit вычисляется по hardware × model × runtime × context с весами, overhead,
KV/cache growth, configured context, headroom и CPU/offload RAM. Recommendation
профили `Fast`, `Balanced`, `Quality`, `LargestCompatible` являются presentation,
не authority.

`Ready` требует process start, expected artifact identity, load, protocol/capability
match, bounded inference probe и отсутствие memory/runtime failure. В Model Gateway
создаётся обычный `provider=local-managed` `ModelProfile` с locality, runtime ref и
capabilities; Context Budget использует active hardware-safe context.

Bootstrap и preferred имеют отдельные exact descriptors. Activation policy:
`Manual`, `PreferWhenReady`, `NewConversationsOnly`; default выбирается на stage 1,
но явный user-selected strict snapshot не меняется manager-ом. Переключение
возможно только между model calls, никогда внутри in-flight call.

## Безопасность и non-goals

- renderer не обнаруживает hardware, не скачивает artifact, не выбирает executable
  и не регистрирует trusted state;
- runtime запускается без shell interpolation, с bounded env/I/O/timeout и без
  provider credentials; process tree supervised и очищается;
- localhost endpoint сам по себе не является managed runtime;
- catalog не выполняет `trust_remote_code`, Python/Node packages или model-supplied
  executable; custom import, если нужен, остаётся отдельным trust level и этапом;
- prompt/output не добавляются в manager diagnostics, а provenance хранит только
  descriptor/revision/hash, runtime version, hardware hash и context/config hash;
- Docker/VM, cloud hosting, training, automatic internet discovery и все backend
  formats кроме выбранного MVP не входят в направление.

## Критерии готовности направления

- [ ] Есть versioned Core-owned hardware snapshot без tracking identifiers.
- [ ] Есть exact revision/hash/capability catalog и allowlisted runtime registry.
- [ ] Есть conservative fit/recommendation с active safe context.
- [ ] Download/verify/atomic-install lifecycle не принимает corrupt/staging state.
- [ ] Runtime supervised и `Ready` выдаётся только после полного health gate.
- [ ] Managed model регистрируется как обычный ModelProfile и участвует в resilience.
- [ ] Bootstrap работает до preferred readiness, а switch соблюдает call/snapshot boundary.
- [ ] Resource policy не выгружает in-flight runtime и восстанавливается после restart.
- [ ] Recovery, diagnostics/provenance, IPC/UI и security evidence воспроизводимы.

## Связанный issue

- [#96 Local Model Runtime Manager](https://github.com/rkfsociety/EvoHime/issues/96)

