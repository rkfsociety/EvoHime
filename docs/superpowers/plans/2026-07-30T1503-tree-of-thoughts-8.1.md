# Tree-of-Thoughts Bounded Planner (8.1) Implementation Plan — REVISED v2

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Agent generates exactly K candidate plans, scores via corrected unified formula, prunes to top-N, executes best one. Fallback to single-plan on error. Store history with score breakdown + TTL cleanup.

**Architecture:** Protocol: unified PlanCandidate + ScoreBreakdown (one per candidate). Storage: planning_history table with validation + TTL cleanup task (respects shutdown signal). Agent-runtime: planning module with LLM access + experience memory retrieval for scoring. Integration: planning phase in agent_loop before tool execution; history saved immediately after planning; configuration-driven parameters (K, top-N, weights, retention).

## Global Constraints

- Backward compatible: if planning fails, fallback to single synthetic plan, continue (no crash).
- `confidence`, `final_score`, all breakdown components must be in [0.0, 1.0] with validation everywhere.
- Scoring formula: `final_score = (w1·similarity + w2·tool_success + w3·(1-complexity) + w4·(1+feedback)/2) / Σw` — guarantees [0.0, 1.0] output when feedback ∈ [-1.0, 1.0].
- Scoring weights + K + top-N + TTL retention_days from centralized config (with defaults).
- Planning phase runs before tool execution, within agent_loop (not deferred to pipeline).
- History saved immediately after planning, regardless of task execution outcome.
- TTL cleanup task respects server shutdown signal (CancellationToken).
- PlanCandidate: unified struct `{id: String, description: String, confidence: f32}` in protocol; no `title`.
- Each PlanCandidate carries its own `score_breakdown: ScoreBreakdown` (not one global breakdown for all).
- Deterministic tie-breaker: sort by `final_score DESC`, then `id ASC` (field name consistent: always `id`, not `plan_id`).
- Migration includes schema-level CHECK on reasoning length + uniqueness on worktree_path.
- LLM client passed to generate_candidate_plans; experience_memory handle passed to score_candidate_plans.
- Reasoning: truncate to 512 chars (not reject); store truncated version.
- One commit per explicit task (numbered 1–8); Quality Checks (Task 7) is separate.

---

## 8 Tasks

**Task 1: Protocol — PlanCandidate + ScoreBreakdown + AgentPlan event**
- Create `crates/protocol/src/planning.rs`:
  - `PlanCandidate { id: String, description: String, confidence: f32 }`
  - `ScoreBreakdown { similarity_score, tool_success_rate, complexity_penalty, feedback_adjustment, final_score }`
  - Implement `is_valid()` on both (confidence/scores in [0.0–1.0])
- Register module in `crates/protocol/src/lib.rs`
- Add `AgentPlan` event to `ServerEvent`:
  - `candidates: Vec<PlanCandidate>` (each with own score_breakdown embedded)
  - `chosen_plan_id: String`
  - `reasoning: String` (max 512 in UI, not enforced here)
- Update `crates/protocol/schema/evohime.protocol.schema.json` with AgentPlan + subschema for PlanCandidate (include score_breakdown fields)
- Run `npm run generate:protocol` and verify no diff on rerun
- Commit: "protocol: add PlanCandidate, ScoreBreakdown, AgentPlan event (8.1)"

**Task 2: Storage — planning_history table + DAO + TTL cleanup**
- Create migration `migrations/0036_planning_history.sql`:
  - Table: id (uuid PK), task_id (FK tasks), session_id (FK sessions), candidates (jsonb array), chosen_plan_id (text), reasoning (text, CHECK length ≤ 512), created_at (default now)
  - Indexes: (task_id), (created_at for TTL cleanup)
- Create `crates/storage/src/planning_history.rs`:
  - `PlanningHistoryEntry { task_id, session_id, candidates, chosen_plan_id, reasoning }`
  - `insert_planning_history(pool, entry)` → validate: confidence ∈ [0.0–1.0], reasoning ≤ 512, truncate reasoning if needed
  - `cleanup_old_planning_history(pool, retention_days)` → delete rows where created_at < now - retention_days, return count
  - `list_planning_by_task(pool, task_id)` → fetch history
- Register in `crates/storage/src/lib.rs`
- Tests: insert/list, confidence validation, reasoning truncation (not rejection)
- Commit: "feat(storage): add planning_history table, DAO, validation (8.1)"

**Task 3: Agent-runtime planning module**
- Create `crates/agent-runtime/src/planning.rs`:
  - `PlanningConfig { num_candidates: usize, top_n: usize, weights: ScoringWeights }`
  - `ScoringWeights { similarity: f32, tool_success: f32, complexity: f32, feedback: f32 }`
  - `generate_candidate_plans(llm_client: &impl LlmClient, experience: &ExperienceHandle, request: &PlanningRequest) → Result<Vec<PlanCandidate>>`
    - LLM generates K plans via structured JSON output
    - Each candidate returned with confidence=0.5 initially (to be updated by scoring)
  - `score_candidate_plans(pool: &PgPool, experience: &ExperienceHandle, candidates: &[PlanCandidate], task_desc: &str, weights: &ScoringWeights) → Result<Vec<(PlanCandidate, ScoreBreakdown)>>`
    - Formula: `final_score = (w1·similarity + w2·tool_success + w3·(1-complexity) + w4·(1+feedback)/2) / Σw`
    - Where feedback ∈ [-1.0, +1.0] (clamped)
    - Returns pair of (candidate with updated confidence, breakdown)
  - `prune_to_top_n(candidates: Vec<(PlanCandidate, ScoreBreakdown)>, top_n: usize) → Vec<(PlanCandidate, ScoreBreakdown)>`
    - Sort by `breakdown.final_score DESC`, then `candidate.id ASC`
- Register in `crates/agent-runtime/src/lib.rs`
- Tests: generation count, validation (invalid scores rejected), tie-breaker, fewer-than-top-n handling
- Commit: "feat(agent-runtime): add planning module with corrected scoring formula (8.1)"

**Task 4a: Integrate planning into agent_loop**
- Modify `crates/agent-runtime/src/lib.rs`:
  - Before tool execution phase, insert planning:
    ```
    // Get config from app_state
    let planning_config = config.planning_config.clone(); // { num_candidates, top_n, weights }
    
    // Generate candidates
    let candidates = match generate_candidate_plans(llm_client, experience_handle, &PlanningRequest{...}) {
        Ok(c) if !c.is_empty() => c,
        _ => {
            // Fallback: single synthetic plan, log warning
            vec![PlanCandidate { id: "default".into(), description: format!("Execute: {}", task_desc), confidence: 0.5 }]
        }
    };
    
    // Score candidates
    let scored = match score_candidate_plans(pool, experience, &candidates, &task_desc, &planning_config.weights) {
        Ok(s) => s,
        Err(e) => {
            // Fallback: equal scores, log warning
            vec![(candidates[0].clone(), ScoreBreakdown::default())]
        }
    };
    
    // Prune to top-N
    let pruned = prune_to_top_n(scored, planning_config.top_n);
    
    // Emit AgentPlan event
    if let Some((chosen, breakdown)) = pruned.first() {
        emit_event(ServerEvent::AgentPlan { candidates: pruned.iter().map(|(c,_)| c.clone()).collect(), chosen_plan_id: chosen.id.clone(), reasoning: "..." });
        
        // **Key**: inject chosen plan into agent context (system message or task context)
        let refined_task = format!("Original: {}\n\nSelected approach: {}", task_desc, chosen.description);
        // Use refined_task in LLM prompts before tool execution
    }
    
    // **Save history immediately (not deferred)**
    let _ = planning_history::insert_planning_history(pool, &PlanningHistoryEntry { ... }).await; // log error, non-fatal
    
    // Tracing
    tracing::info!(task_id=%task_id, chosen_plan=%chosen.id, confidence=chosen.confidence, "planning phase complete");
    ```
  - Test: fallback when generation fails, history saved even on task failure
- Commit: "feat(agent-runtime): integrate planning phase into agent_loop with fallback (8.1)"

**Task 4b: Explicit Task 4c — TTL cleanup loop in startup**
- Modify `crates/server/src/startup.rs`:
  - After other initialization, spawn cleanup task with shutdown signal:
    ```rust
    let shutdown_rx = shutdown_signal.subscribe(); // Existing pattern in codebase
    let pool = pool.clone();
    let retention_days = app_config.planning_retention_days; // From config
    
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(86400)); // 24h
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = planning_history::cleanup_old_planning_history(&pool, retention_days).await {
                        tracing::error!("planning history cleanup failed: {}", e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("planning history cleanup loop shutting down");
                    break;
                }
            }
        }
    });
    ```
- Commit: "feat(server): add TTL cleanup loop for planning history (8.1)"

**Task 5: Frontend — AgentPlanView component**
- Create `frontend/web/src/components/AgentPlanView.tsx`:
  - Props: `candidates: PlanCandidate[]`, `chosenPlanId: string`, `reasoning: string`
  - Validate: empty candidates → return null; chosen_plan_id not in candidates → log, render with warning
  - Render: list of candidates with highlight on chosen; show each candidate's confidence %; collapsible <details> for each candidate's score breakdown
  - Handle edge case: if `score_breakdown` missing or values out of range → show "N/A" or clamped display
  - CSS: 44px+ touch targets, dark mode aware
- Integrate into `frontend/web/src/panels/ChatPanel.tsx`:
  - Add handler for `ServerEvent::AgentPlan` in message renderer
- Build: `npm run build` (no TS errors)
- Commit: "ui: add AgentPlanView component with per-candidate score breakdown (8.1)"

**Task 6: E2E Integration Test**
- Create `crates/server/tests/planning_e2e.rs`:
  - Test: user sends task → planning generates K candidates → scores them → prunes to top-N → emits AgentPlan event with correct candidates + chosen_id → saves planning_history row
  - Assertions: candidates.len() == K; all confidence ∈ [0.0–1.0]; chosen_plan_id matches first candidate; reasoning ≤ 512; planning_history row exists with matching data
  - Can use mock LLM (structured output stub) and fake experience_handle if live DB unavailable
  - Document: expected flow "plan generated → plan executed → history saved"
- Commit: "test(server): add end-to-end planning flow test (8.1)"

**Task 7: Quality Checks**
- Run: `cargo test --workspace --exclude evohime-installer --exclude evohime-artifacts` (all pass)
- Run: `cargo clippy -p evohime-protocol -p evohime-agent-runtime -p evohime-storage -p evohime-server -- -D warnings` (clean)
- Run: `cargo fmt --all -- --check` (clean)
- Run: `npm run build` (succeeds)
- Run: `npm run generate:protocol` (no diff)
- No commit (verification only)

**Task 8: Documentation**
- Update `docs/roadmap.md`: mark `8.1` ✅, add evidence (protocol + DAO + formula + fallback + history + TTL + E2E test)
- Update `docs/current-state.md`: add Stage 8.1 description
- Update `AGENTS.md`: "8.1 ✅ — Tree-of-Thoughts: K candidate plans, unified scoring (similarity + tool success + complexity + feedback), deterministic pruning to top-N, fallback on error, history with 30-day TTL"
- Commit: "docs: mark 8.1 Tree-of-Thoughts bounded planner complete (8.1)"

---

## Configuration (Global)

All parameters loaded from `AppConfig` or similar:
- `planning_num_candidates: usize` (default: 3) — K
- `planning_top_n: usize` (default: 3) — top-N for pruning
- `planning_weights: ScoringWeights` (default: {similarity: 0.3, tool_success: 0.3, complexity: 0.2, feedback: 0.2})
- `planning_retention_days: i32` (default: 30) — TTL cleanup

Example in config schema or env:
```
EVOHIME_PLANNING_NUM_CANDIDATES=3
EVOHIME_PLANNING_TOP_N=3
EVOHIME_PLANNING_RETENTION_DAYS=30
```

---

## Scoring Formula — Corrected

**Input validation:**
- `similarity_score ∈ [0.0, 1.0]`
- `tool_success_rate ∈ [0.0, 1.0]`
- `complexity_penalty ∈ [0.0, 1.0]`
- `feedback_adjustment ∈ [-1.0, 1.0]`

**Aggregation:**
```
final_score = (w1·similarity + w2·tool_success + w3·(1 - complexity) + w4·(1 + feedback)/2) / sum(weights)
```

**Verification:**
- Best case: all 1.0, feedback 1.0 → `(0.3·1 + 0.3·1 + 0.2·1 + 0.2·1) / 1.0 = 1.0` ✓
- Worst case: all 0.0, feedback -1.0 → `(0 + 0 + 0.2·1 + 0.2·0) / 1.0 = 0.2` ✓
- Always in [0.0, 1.0] ✓

---

## Key Implementation Details

1. **LLM structured output** (Task 3): `generate_candidate_plans` calls `llm_client.complete_structured(prompt, schema)` returning JSON array of plans.
2. **Experience memory** (Task 3): `score_candidate_plans` calls `experience.search_similar(task_desc)` to get similarity_score; queries tool history for tool_success_rate.
3. **Plan transmission** (Task 4a): refined_task with chosen plan description injected into agent context (system message or task field) before tool execution resumes.
4. **History storage** (Task 4a): `insert_planning_history` called immediately after planning phase completes, before task execution. Non-fatal on error.
5. **Fallback**: if ANY stage fails (generation, scoring, pruning, history save), use single synthetic plan with description="Execute: {original task}"; log warning; continue normally.
6. **Tie-breaker** (Task 3): sort by `score DESC, id ASC` — deterministic, stable across runs.

---

## Verification Checklist

- [ ] All 8 tasks committed (separate, ordered)
- [ ] `cargo test` passes (Task 7)
- [ ] `cargo clippy` clean (Task 7)
- [ ] `npm run build` succeeds (Task 7)
- [ ] Protocol drift check passes (Task 7)
- [ ] All confidence/score values validated [0.0–1.0]
- [ ] Scoring formula verified (always [0.0–1.0] output)
- [ ] History saved immediately in agent_loop (not deferred)
- [ ] TTL cleanup respects shutdown signal
- [ ] Fallback tested for all error paths
- [ ] LLM client + experience_memory parameters in function signatures
- [ ] Config-driven parameters (K, top-N, weights, TTL)
- [ ] Reasoning truncated, not rejected
- [ ] chosen_plan_id validation in frontend

---

**All 19 review comments + 20 prior comments addressed. Ready for fresh review or implementation.**
