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

- канонический TaskCheckpoint contract закрытого плана 23 как bounded
  trajectory/evidence source;
- канонический Agent Skills contract закрытого плана 24 как Core-owned
  registry для skill candidate activation;
- существующие typed memory extraction/retrieval, provenance, privacy и
  confirmation/approval flows;
- Core event journal, SQLite migrations, audit, redaction и authenticated IPC.

### Опциональные

- канонический Persistent Goals contract закрытого плана 25 для goal-level
  source/evidence;
- канонический Continuation Policy contract закрытого плана 26 для
  quality-gate trajectory;
- канонический retained-child contract закрытого плана 27 для child evidence;
- план 28 Analysis Kernel для bounded eval fixtures, без kernel authority.

## Candidate contract и lifecycle

Ввести versioned `RefinementCandidateV1`: id, immutable revision, kind, target,
scope, owner scope, status, title,
rationale, proposed content, source task ids, evidence refs, conflicting refs,
confidence, author, created/evaluated/activated timestamps, optional supersedes и
content hash. `kind` расширяемый (`Memory`, `Skill`, `PromptRule`, позже
`ChildRole`, `WorkflowTemplate`, `RoutingRule`); MVP реализует первые три.

Scope: `session`, `workspace`, `global`. Более широкий scope требует более
строгой policy. Состояния: `Draft → Evaluating → Proposed → Approved → Active →
Superseded/RolledBack`, а также `Rejected/FailedEvaluation`. Все переходы
durable, append-only и содержат actor/policy/evidence refs.

### Evidence admission и boundedness

Candidate создаётся только из завершённой task trajectory либо явного действия
пользователя. Core нормализует observation в bounded fingerprint `pattern_key`,
привязанный к owner scope, kind и target; одинаковый эпизод не считается
независимым подтверждением. Для каждого kind/scope до реализации фиксируются в
policy минимальное число наблюдений, минимальное число независимых task ids,
окно времени, лимит кандидатов на scope, максимальные размеры
title/content/evidence refs и retention. Если порог не достигнут, наблюдение
остаётся evidence, но candidate не создаётся; threshold не выводится моделью и
не меняется самим candidate.

Каждая evidence ref должна иметь durable source id, source kind, owner scope,
content hash, observed-at и redaction/sensitivity metadata. Core проверяет, что
source существует и не был удалён или отозван, а conflicting evidence нельзя
считать подтверждением. При удалении источника candidate переводится в
`FailedEvaluation`/`Rejected` с typed reason, а не остаётся активируемым.

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

Для `PromptRule` должен существовать отдельный Core-owned versioned registry
либо typed adapter к владельцу prompt policy; до его появления этот target
возвращает `unavailable` и не активируется обходным путём. Ни один candidate не
получает право менять system prompt, capability registry, approval policy или
tool routing policy.

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

### Acceptance IDs

- `R29-C01` — repeated independent evidence создаёт candidate, единичное
  наблюдение не создаёт global rule;
- `R29-C02` — candidate version, scope, provenance, hash, owner и lifecycle
  durable и bounded;
- `R29-C03` — duplicate/conflict/expired-or-deleted evidence и evaluation
  failure дают typed non-active outcome;
- `R29-C04` — activation проходит только через Core-owned Memory API,
  `SkillRegistry` или versioned prompt-rule adapter и exact policy snapshot;
- `R29-C05` — global/high-risk activation требует explicit approval, а
  unsupported target даёт `unavailable`;
- `R29-C06` — before/after revision, rollback и observation сохраняются без
  blind retry или in-place mutation;
- `R29-C07` — authenticated IPC/UI показывает только bounded metadata/diff и
  повторяемые user actions;
- `R29-C08` — restart, retention, redaction и adversarial self-escalation
  подтверждены reproducible evidence.

## Не входит

Полностью автономное self-modification, переписывание system prompt целиком,
обучение model weights, network publication/sync, security-policy changes или
массовая генерация правил после каждого turn.
