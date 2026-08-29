# План 29.0 — Continual Refinement с evidence и approval

Статус: предложено по [issue #4](https://github.com/rkfsociety/EvoHime/issues/4).
Это контролируемый proposal/evaluation pipeline, а не autonomous self-modifying
режим.

## Цель и граница

Превратить повторяющиеся ошибки, пользовательские коррекции и устойчивые
рабочие паттерны в проверяемые предложения для memory, skills и prompt rules.
Поток: `trajectory → reflection → candidate → evaluation → approval/policy →
activation → observation/rollback`. Агент не может через refinement расширить
свои tools, grants, credentials, approvals или security policy.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./29-1-continual-refinement.md)
- [Этап 2 — runtime-интеграция и recovery](./29-2-continual-refinement.md)
- [Этап 3 — IPC, client projection и UI](./29-3-continual-refinement.md)
- [Этап 4 — verification, release-evidence и закрытие](./29-4-continual-refinement.md)

## Зависимости

### Блокирующие

- план 23 TaskCheckpoint как bounded trajectory/evidence source;
- план 24 Agent Skills как Core-owned registry для skill candidate activation;
- существующие typed memory extraction/retrieval, provenance, privacy и
  confirmation/approval flows;
- Core event journal, SQLite migrations, audit, redaction и authenticated IPC.

### Опциональные

- план 25 Persistent Goals для goal-level source/evidence;
- план 26 Continuation Policy для quality-gate trajectory;
- план 27 retained children для child evidence;
- план 28 Analysis Kernel для bounded eval fixtures, без kernel authority.

## Candidate contract и lifecycle

Ввести versioned `RefinementCandidateV1`: id, kind, scope, status, title,
rationale, proposed content, source task ids, evidence refs, conflicting refs,
confidence, author, created/evaluated/activated timestamps, optional supersedes и
content hash. `kind` расширяемый (`Memory`, `Skill`, `PromptRule`, позже
`ChildRole`, `WorkflowTemplate`, `RoutingRule`); MVP реализует первые три.

Scope: `session`, `workspace`, `global`. Более широкий scope требует более
строгой policy. Состояния: `Draft → Evaluating → Proposed → Approved → Active →
Superseded/RolledBack`, а также `Rejected/FailedEvaluation`. Все переходы
durable, append-only и содержат actor/policy/evidence refs.

Reflection получает Core-owned projection: objective, decisions, tool failures,
user corrections, test/gate results, child evidence, result и активные
memory/skill/rule metadata. Raw transcript, credentials и новые capabilities не
передаются. Единичный эпизод не создаёт global rule без explicit user action.

## Evaluation и activation policy

Перед activation проходят schema/contract, size/complexity, duplicate, conflict,
scope, security и behavior evaluation. Для skill/prompt rule сравниваются
baseline/candidate на frozen fixtures. Weak evidence оставляет candidate в
`Proposed`; conflict блокирует auto-activation. Candidate не может предложить
credential, отключить approval, расширить grant, включить unrestricted
shell/network или изменить security policy.

Рекомендуемая policy: low-risk session memory может применяться автоматически;
workspace memory требует user approval либо строгой reversible policy; global
memory/skill/prompt rule требует явного подтверждения. Любая запись проходит
`SkillRegistry`/memory API и не пишет файл или registry напрямую из model output.

Activation сохраняет before/after version, content hash, actor, evidence,
evaluation result и rollback ref. Наблюдение после activation может создать
новый candidate, но не переписывает активную запись на месте.

## Persistence, privacy и UI

Хранить candidate/revision/evaluation/activation/rollback в additive durable
tables; content/raw evidence — только по bounded refs и sensitivity policy.
Renderer получает type/scope/title/evidence count/conflicts/confidence/eval
status/diff и approve/reject/rollback actions. Raw sensitive payload и
необезличенный transcript не выдаются.

В OperationsPanel добавить очередь и историю refinement candidates. Approve,
reject, activate и rollback идемпотентны, повторно проверяют version/conflict и
пишут audit. Activation после restart должна сохранять provenance и безопасно
останавливаться при schema/runtime mismatch.

## Этапы реализации

1. Зафиксировать candidate schema, scope/approval matrix, forbidden changes и
   provenance model.
2. Реализовать bounded reflection projection и durable candidate/evidence store.
3. Добавить duplicate/conflict/scope/security checks и frozen behavior evals.
4. Подключить approve/reject/activate/rollback к Memory API и SkillRegistry,
   затем добавить typed IPC/UI history.
5. Провести adversarial tests на self-escalation, weak evidence, conflicts,
   sensitive renderer output, restart и failed evaluation.

## Критерии готовности

- repeated evidence создаёт candidate, единичная ошибка не создаёт global rule;
- candidate имеет scope, provenance, content hash и durable lifecycle;
- duplicate/conflict/security/eval failure блокируют unsafe activation;
- global activation требует explicit approval;
- activated versions имеют rollback и before/after history;
- refinement не расширяет capabilities и не меняет security policy;
- UI показывает bounded queue/history/diff без sensitive raw content;
- provenance переживает restart, а tests покрывают memory/skill/prompt paths.

## Не входит

Полностью автономное self-modification, переписывание system prompt целиком,
обучение model weights, network publication/sync, security-policy changes или
массовая генерация правил после каждого turn.
