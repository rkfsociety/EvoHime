# Agent memory, experience, and ask-on-uncertainty self-learning

> Дата: 2026-07-16  
> Статус: approved  
> Roadmap: `6.16`–`6.25`

## Goal

Replace free-text `session_memory` / `global_memory` notes with a structured, automatic memory system for **local single-tenant** use.

EvoHime does **not** fine-tune model weights. “Self-learning” means accumulating facts, constraints, failure/success patterns, and playbooks, then retrieving them into bounded agent context.

Default posture: **the agent decides alone**. If confidence is low or impact is high, it **asks** once; the Memory panel is for transparency and override, not a mandatory approve step on every write.

## Non-goals (v1)

- Multi-user ACL, team sharing, org scopes
- Embedding / semantic retrieval — `6.25` local feature-hash hybrid; optional remote neural via `EVOHIME_EMBEDDING_MODE=remote` (OpenAI-compatible `/embeddings`)
- Mandatory human approval for every extracted item
- Changing LLM weights or training pipelines

## Deployment assumptions

- One human operator on one machine
- `user` scope is an alias of `global` (local operator profile)
- Hard isolation is **workspace/project**, not multi-tenant users
- Local backup/export may exist later; collaboration sync will not

## Scopes

| Scope | Meaning |
| --- | --- |
| `session` | Ephemeral notes for the current session/task |
| `workspace` | Bound to workspace root identity (path + optional git remote id) |
| `project` | Logical project; v1 may equal workspace |
| `global` | Operator habits, style, standing constraints on this machine |
| `experience` | Reusable success/failure/verify/playbook patterns; promote to `global` only at high confidence |

Workspace identity must not rely on bare path alone when a git remote is available, so relocated checkouts do not silently orphan or leak memory.

## Item model

Each memory item at minimum:

| Field | Purpose |
| --- | --- |
| `id` | Stable id |
| `scope` + `scope_key` | Where it applies |
| `kind` | `fact`, `preference`, `constraint`, `failure_pattern`, `success_pattern`, `verification_rule`, `playbook`, … |
| `status` | `candidate`, `active`, `conflict`, `archived`, `rejected` |
| `content` | Normalized text / structured payload |
| `confidence` | 0..1 extraction/decision confidence |
| `importance` | Ranking weight |
| `source` | Task/session/tool provenance |
| `supersedes` | Optional prior item id |
| `pinned` | Sticky; resists decay (set only via ask or rare high-confidence global rule) |
| `valid_until` / validity hints | Optional time or branch-bound validity |
| `created_at` / `updated_at` | Audit |

### Status semantics

- `candidate` — weak influence in retrieval (low weight), never treated as system law
- `active` — normal retrieval weight
- `conflict` — retained for UI/resolution; **excluded** from prompt until resolved/superseded
- `archived` / `rejected` — not retrieved

## Automatic lifecycle

```text
task completed | task failed | explicit “remember” signal
  → extract (strict JSON schema via model-gateway)
  → redact secrets
  → normalize + dedupe
  → conflict detect
  → write as candidate
  → decision gate
       ├ high confidence + safe heuristics → auto-promote → active
       ├ uncertain / conflict / high-impact → ask operator
       └ secret / garbage / failed validation → drop or archive
```

### Decision gate (ask-on-uncertainty)

Ask when any of:

- Conflict with an `active` item
- Claim wants `global`, `pinned`, or hard `constraint`
- High-impact security / irreversible policy
- Low confidence or failed verification linkage
- Ambiguous workspace/project binding

Otherwise auto-promote.

Ask UX: short chat/modal prompt (“Remember X as project fact?” → yes / no / edit).  
Asking must **not** freeze unrelated tool work forever: deferrable promote is allowed; item stays `candidate` until answered.

### Safety rails (always on)

1. Secrets, tokens, passwords, cookies, private keys are never stored
2. Retrieved memory is injected as **untrusted data**, below system rules and the current user request
3. Workspace/project memory never cross-contaminates another workspace
4. Prompt budget with priority: pinned → active → experience → weak candidate
5. One failed task must not silently rewrite `global`
6. Operator can delete/edit/pin/forbid any item in the Memory panel without blocking the happy path

## Retrieval

- Lexical / structured retrieval (`6.19`) plus hybrid embeddings (`6.25`): feature-hash by default, optional remote neural encoder
- Auto-inject top-N into agent context under token budget
- Optional tool `memory.search` / `memory.recall` for on-demand lookup
- Attribution: `used_memory_ids` recorded for feedback and debugging
- System instructions and the live user message always outrank memory
- Pins sort above score so sticky operator rules stay in prompt first

## Feedback loop

Automatic signals:

| Signal | Effect |
| --- | --- |
| Task success + memory used | Soft helpful / confidence bump |
| Task fail + memory used | Decay and/or mark suspect |
| Operator corrects on ask | Write corrected active; reject prior |
| Operator rejects on ask | No promote; optional negative constraint |
| Prolonged unused + low confidence | Decay toward archive |

## Experience / playbooks (`6.21`)

Structured kinds beyond free text:

- Success patterns (“when X, do Y”)
- Failure patterns (“avoid Z because …”)
- Verification rules (“prove done by …”)
- Playbooks: `trigger → steps → verify → rollback hint`

Auto-extracted playbooks start as `candidate`; promote follows the same gate.

## UI (`6.22` / `6.24`)

Panel is **override and observability**, not a queue of mandatory approvals:

- Active / candidates / experiences / conflicts
- Edit, reject, archive, delete, pin
- Privacy: show redaction policy; no secret storage toggle that weakens redaction
- Optional local export/backup later (single operator migration)

## Migration

- Import existing `session_memory` / `global_memory` notes into structured items as `candidate` (or low-confidence `active` for global deduped notes after redaction)
- Keep read compatibility until cutover completes

## Crate / protocol shape

| Piece | Location |
| --- | --- |
| Schema | `migrations/` |
| Persistence API | `crates/storage/` |
| Domain service | new `crates/memory/` |
| Extract + inject | `crates/agent-runtime/`, `crates/model-gateway/` |
| Ask + events | `crates/protocol/`, `crates/server/`, `frontend/web/` |
| Panel | `frontend/web/` |

Suggested events (names indicative): `memory.proposed`, `memory.ask`, `memory.accepted`, `memory.rejected`, `memory.used`, `memory.conflict`.

## Implementation order

1. `6.16`–`6.17` schema + scopes (+ migrate legacy notes)
2. `6.18` memory service (redact, dedupe, conflict)
3. `6.19` retrieval + budget + untrusted tagging (+ `memory.search`)
4. `6.20` extraction + decision gate (ask-on-uncertainty)
5. `6.21` experience/playbook kinds
6. `6.23` feedback / decay
7. `6.22`/`6.24` Memory panel overrides
8. `6.25` embeddings only after lexical quality is proven

## Acceptance criteria

- Single-tenant local operator; no multi-user memory ACL required
- Scopes: session, workspace, project, global(=user), experience
- Default path is automatic; ask only on uncertainty / high impact
- Candidates never act as system law; conflicts stay out of prompt
- Secrets never persist; workspace isolation holds
- Retrieval is budgeted and attributed (`used_memory_ids`)
- Operator can override any item without being in the critical path for routine writes
- Storage, integration, and redaction/security tests cover the flow
- Embeddings are explicitly out of v1 acceptance

## Out of scope follow-ups

- Multi-device sync beyond optional local export
- Team playbook marketplace
- Online continual fine-tuning
