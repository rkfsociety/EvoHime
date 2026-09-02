# План 119.0 — Execution Environment Profiles: атомарные operating modes и effective snapshots

Статус: предложено по [issue #99](https://github.com/rkfsociety/EvoHime/issues/99). Это обзорный план направления; реализация начинается после отдельного evidence review. Закрытие issue означает перенос требований в этот исполнимый план, а не готовность функционала.

## Цель

Добавить в EvoHime Core-owned **Execution Environment Profile** — именованный versioned профиль, который композирует ссылки на уже существующие Model/MCP/Workbench/Skill/Instruction/Policy/External Agent подсистемы и активируется как одна проверяемая конфигурация.

Профиль устраняет промежуточные операционно бессмысленные состояния вроде `local model + cloud MCP + offline policy`, но не создаёт вторую базу настроек и не заменяет authoritative registries.

## Текущее основание и граница

В checkout уже есть ModelProfile/Model Purpose Routing, Schema-Driven Agent Configuration, Customization Inventory, MCP/Workbench lifecycle, Skills, Project Instruction Stack, execution/approval/continuation policies, External Coding Agent Adapter, budget policies и Declarative Runtime Components. Новый слой хранит только typed refs/options и activation history; semantics и state остаются у владельцев.

Кандидатные поверхности: `crates/evohime-core/src/execution_environment_profiles.rs`, local-storage, resolver/activation integration, authenticated desktop IPC, Electron main/preload/renderer и canonical docs. Точные имена, schema revision и IPC tags подтверждаются на evidence freeze по live checkout.

## Граница сущностей

```text
ModelProfile / RoutingPolicy      = model choice and purpose routing
CustomizationInventory            = available extensions/catalog
Workbench/MCP                     = tool definition and lifecycle
Skill/Instruction Stack            = context and behavior inputs
Execution/Approval/Budget Policy  = hard limits and authorization
ExternalAgentPreset               = known adapter configuration
ExecutionEnvironmentProfile       = composition of refs + activation semantics
EffectiveEnvironmentSnapshot      = immutable resolved run/turn state
```

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./119-1-execution-environment-profiles.md)
- [Этап 2 — resolver, activation и recovery](./119-2-execution-environment-profiles.md)
- [Этап 3 — IPC, client projection и UI](./119-3-execution-environment-profiles.md)
- [Этап 4 — verification, release-evidence и закрытие](./119-4-execution-environment-profiles.md)

## Зависимости

### Блокирующие

- Model Purpose Routing/ModelProfile, Execution Backend/External Coding Agent Adapter.
- Schema-Driven Agent Configuration, Customization Inventory, Skills, Project Instruction Stack и Declarative Runtime Components.
- MCP/Workbench lifecycle, Execution/Approval/Continuation/Budget policies, data-sensitivity policy, credential slots, event journal, SQLite migrations и authenticated IPC.

### Опциональные

- Local Model Runtime Manager — дополнительные local profiles и hardware availability.
- Typed Context References/Context Mentions — environment refs в conversation context.
- Diagnostics & Support Bundle — расширенный redacted activation evidence.

## Основной контракт направления

Core вводит versioned `ExecutionEnvironmentProfile`, `EnvironmentBinding`, `EnvironmentProfileDiff`, `EnvironmentActivation` и `EffectiveEnvironmentSnapshot`. Binding kinds bounded: `ModelRouting`, `DefaultModelProfile`, `ExternalAgentPreset`, `Workbench`, `McpServer`, `SkillSet`, `InstructionStack`, `ExecutionPolicy`, `ApprovalPolicy`, `ContinuationPolicy`, `BudgetPolicy`, `CredentialBinding`.

Scopes первого этапа: `Application`, `Workspace`, `Project`, `ConversationDefault`. Bindings явно `required` или optional. Required failure блокирует activation целиком; недоступный optional binding даёт явный `Degraded`, а не скрытое частичное переключение.

Derived profile states: `Ready`, `NeedsReview`, `Degraded`, `Broken`. Profile revisions immutable; refs поддерживают `PinnedRevision` и только явно versioned `FollowCompatible`. Перед активацией Core выполняет resolve → preflight → compatibility/data-boundary/security checks → diff → safe-boundary activation → audit event.

`NewRunOnly`, `NextTurn`, `NewConversationOnly` выбираются Core по binding composition. Каждый run/eligible turn получает immutable effective snapshot с exact profile/revision/hash, resolved refs, policy snapshots, credential slot refs и content hash. Смена профиля не мутирует активную работу.

Rollback — новая activation event предыдущего валидного snapshot/profile, не изменение истории. Secret values не входят в profile, export, diff, IPC или renderer.

## Security и non-goals

Environment Profile не является capability grant и не может ослабить hard security/data-boundary ceiling. Every ref повторно валидируется owner subsystem; imported profile не активируется автоматически. External agent materialization идёт только через known adapter, без произвольных `$HOME` config writes, scripts или executable hooks. LocalOnly workspace блокирует cloud binding.

Не входят generic editor чужих конфигов, новый plugin/runtime/marketplace, второй model/MCP/skills/budget registry, arbitrary scripts, automatic network discovery, secret export и hot mutation уже выполняющегося immutable run.

## Критерии готовности направления

- [ ] Есть versioned Core-owned profile из typed refs/options без дублирования state.
- [ ] Required/optional bindings и Ready/NeedsReview/Degraded/Broken вычисляются Core-side.
- [ ] Preflight выдаёт typed missing/capability/policy/scope/data-boundary diagnostics.
- [ ] Required activation all-or-nothing, optional degradation явна.
- [ ] Activation соблюдает safe boundary и не меняет active run snapshot.
- [ ] Effective snapshot/profile revision/hash фиксируются в run provenance.
- [ ] Drift, pinned/follow-compatible и rollback имеют deterministic history.
- [ ] Credential slots не содержат secret material; imports требуют validation/rebinding.
- [ ] UI/CLI используют те же Core commands и остаются projection-only.

## Связанный issue

- [#99 Execution Environment Profiles](https://github.com/rkfsociety/EvoHime/issues/99)
