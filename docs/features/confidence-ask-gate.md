# Stage 8.4: Meta-cognitive Confidence Ask-Gate

**Status:** Core infrastructure complete  
**Last updated:** 2026-08-03

## Overview

The Confidence Ask-Gate is a multi-signal decision system that determines whether the agent should proceed with a planned action, ask for confirmation, or require explicit approval based on computed confidence and risk levels.

Instead of a binary "ask or don't ask" gate, this system evaluates **5 independent signals**:

1. **Model Confidence** — how confident the LLM itself is (from logprobs, thinking tokens, or heuristics)
2. **Experience Alignment** — how similar the current task is to past successful solutions
3. **Tool Success Rate** — historical success rate of the tools planned (smoothed Beta-binomial prior)
4. **Reflection Confidence** — feedback from self-reflection stage on agent's own error patterns
5. **Risk Level** — discrete assessment of how dangerous the planned operations are

## Architecture

### Risk Levels (Independent from Confidence)

Risk is assessed **independently** from confidence. Planned operations are classified into four levels:

| Level | Description | Examples |
|-------|-------------|----------|
| **None** | Read-only operations | `filesystem.read`, `git.status`, `browser.open` |
| **Low** | Safe writes | Creating files in temp/logs, `git.pull`, HTTP GET |
| **Medium** | Code modifications | `filesystem.patch`, `git.commit`, MCP calls |
| **High** | Destructive/repo operations | `git.push`, `shell.execute`, risky shell commands |

### Confidence Score Calculation

All five signals are weighted and aggregated:

```
confidence = 0.35 * model
           + 0.25 * experience
           + 0.25 * tools
           + 0.15 * reflection
           - penalties_by_reliability
```

**Weights always sum to 1.0.** Penalties are applied per-signal based on reliability level:

| Reliability | Penalty |
|-------------|---------|
| High | 0.0 |
| Medium | -0.05 |
| Low | -0.10 |
| VeryLow | -0.15 |

### Ask Decision Policy

**Risk and Confidence are orthogonal axes.** The decision logic:

```
if risk >= High:
    if confidence < require_threshold:
        return RequireApproval
        
if missing_signals >= 2:
    if confidence < missing_signal_threshold:
        return RequireApproval
    elif confidence < ask_threshold:
        return Ask

if confidence >= thresholds[risk].proceed:
    return Proceed
elif confidence >= thresholds[risk].ask:
    return Ask
else:
    return RequireApproval
```

### Risk-Aware Thresholds

Thresholds **change based on risk level** (configurable via `EVOHIME_CONFIDENCE_THRESHOLDS`):

```json
{
  "none": {"proceed": 0.65, "ask": 0.40, "require": null},
  "low": {"proceed": 0.70, "ask": 0.45, "require": null},
  "medium": {"proceed": 0.75, "ask": 0.50, "require": null},
  "high": {"proceed": 0.85, "ask": 0.65, "require": 0.30}
}
```

**High-risk tasks ALWAYS require approval if confidence ≤ require threshold.** Low-risk tasks proceed with lower confidence.

## Signal Details

### 1. Model Confidence

Extracted from LLM completion using priority fallback:

1. **Logprobs** (if available from provider) → **High reliability**
   - Average of token log-probabilities
   - Only OpenAI-compatible providers exposing logprobs

2. **Structured Output** (explicit confidence field) → **Medium reliability**
   - LLM instructed to output `{"confidence": 0.8}`
   - Requires system prompt modification

3. **Thinking Token Ratio** → **Low reliability**
   - For Claude: longer thinking ≠ confidence (it means the model was thorough)
   - Heuristic: `1.0 - clamp(|thinking% - 30%| / 30%, 0, 1)`
   - Note: thinking depth ≠ confidence

4. **Keyword Heuristics** → **Very Low reliability**
   - Count uncertain keywords ("maybe", "perhaps", "I'm not sure")
   - Count confident keywords ("definitely", "certainly")
   - Score = confident_count / total_count

5. **Fallback** → **Low reliability**
   - Neutral 0.5 if no signals available
   - Does NOT boost confidence artificially

### 2. Experience Alignment

Retrieved from memory system during plan creation:

- Query similar playbooks/past solutions (cosine similarity > 0.65)
- Take top-3 matches weighted by (similarity × confidence_at_creation × recency)
- If <2 similar examples: alignment = 0.5 (uncertain)
- Else: alignment = weighted_mean([example.confidence for example in matches])

**Reliability:** Medium (depends on memory quality)

### 3. Tool Success Rate

Historical statistics from `tool_execution_stats` table:

- **Tracking:** every tool execution records (tool_name, success, error_category, created_at)
- **Calculation:** (success_count + 1) / (total_count + 2) — Beta-binomial prior
  - Prevents extreme confidence on small sample sizes
  - Success rate is **per-tool**, not aggregated
  
- **Reliability tiers:**
  - High: total_count ≥ 5
  - Medium: 1 ≤ total_count < 5
  - Low: total_count = 0

- **Destructive tools separate:** `git.push` / `filesystem.write` stats don't mix with `grep` / `git.status`
- **Multi-tool plans:** conservative **minimum** (worst tool's rate) used

### 4. Reflection Confidence

From `reflection_events` table populated during agent execution:

- Tracks revision_type (minor, major, repeated_failure)
- Formula: `1.0 - clamp(revision_count / (step_count + 1), 0, 1)`
- **Decay:** older revisions fade exponentially
- Repeated failures → lower confidence
- Note: normal iteration (1-2 revisions) is expected; >3 revisions in a row → ask

**Reliability:** Medium (only known after execution starts)

### 5. Risk Level

Deterministic classification of planned steps:

- **None:** all steps are read-only
- **Low:** file creation in safe locations (temp/, logs/, .evohime/)
- **Medium:** code patches, git commits, network calls
- **High:** git push, shell.execute, dangerous patterns (rm -rf, dd if=, mkfs)

## Database Schema

### tool_execution_stats
```sql
CREATE TABLE tool_execution_stats (
    id BIGSERIAL PRIMARY KEY,
    tool_name VARCHAR(50) NOT NULL,
    operation_type VARCHAR(100),
    success BOOLEAN NOT NULL,
    error_category VARCHAR(50),
    task_id UUID NOT NULL,
    workspace_path TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    metadata JSONB
);
```

### confidence_audit_log
```sql
CREATE TABLE confidence_audit_log (
    id BIGSERIAL PRIMARY KEY,
    event_id UUID UNIQUE,
    task_id UUID NOT NULL,
    session_id UUID,
    confidence_score FLOAT NOT NULL,
    risk_level VARCHAR(20) NOT NULL,
    confidence_version VARCHAR(10),
    breakdown JSONB,        -- {model, experience, tools, reflection}
    reliability_scores JSONB,
    missing_signals TEXT[],
    decision VARCHAR(20),   -- proceed | ask | require_approval
    force_approved BOOLEAN,
    force_approval_reason TEXT,
    timestamp TIMESTAMPTZ NOT NULL
);
```

### reflection_events extensions
```sql
ALTER TABLE reflection_events ADD COLUMN IF NOT EXISTS
    revision_type VARCHAR(50) CHECK (revision_type IN ('minor', 'major', 'repeated_failure'));
ALTER TABLE reflection_events ADD COLUMN IF NOT EXISTS
    confidence_delta NUMERIC(3,2);
```

### memory_items extensions
```sql
ALTER TABLE memory_items ADD COLUMN IF NOT EXISTS
    model_confidence_at_creation NUMERIC(3,2) DEFAULT 0.5;
```

## API Endpoints

### GET /api/confidence/audit/task/:task_id
Returns all confidence computations for a task (in audit trail order).

### GET /api/confidence/audit/session/:session_id
Returns last 1000 confidence computations for a session.

### GET /api/settings/confidence-thresholds
Returns current threshold configuration.

### PUT /api/settings/confidence-thresholds
Update thresholds (validated, persisted to settings).

## WS Events

### agent.confidence

Emitted whenever confidence is computed (before decision):

```json
{
  "type": "agent.confidence",
  "task_id": "uuid",
  "timestamp": "2026-08-03T...",
  "confidence_version": "1",
  "confidence_score": 0.72,
  "risk_level": "medium",
  "breakdown": {
    "model": {"score": 0.8, "reliability": "high", "source": "logprobs"},
    "experience": {"score": 0.6, "reliability": "medium"},
    "tools": {"score": 0.7, "reliability": "medium"},
    "reflection": {"score": 0.75, "reliability": "medium"}
  },
  "reliability": {
    "model": "high",
    "experience": "medium",
    "tools": "medium",
    "reflection": "medium"
  },
  "missing_signals": [],
  "recommendation": "ask"
}
```

## Frontend Components

### ConfidenceAndRisk

Displays confidence bar and risk badge side-by-side:

```typescript
<ConfidenceAndRisk
  confidenceScore={0.72}
  riskLevel="medium"
  breakdown={result.breakdown}
  reliability={result.reliability}
  missingSignals={result.missing_signals}
  recommendation="ask"
/>
```

Features:
- Confidence bar (0-100%) with color gradient (red → yellow → green)
- Breakdown grid (4 signals, each with score + reliability)
- Risk badge (✓ Safe / ⚠ Low / ⚠ Medium / 🔴 High)
- Missing signals list (if any)
- Recommendation label (PROCEED / ASK / REQUIRE APPROVAL)

### ForceApproveModal

High-risk override modal (high-risk + low-confidence only):

```typescript
<ForceApproveModal
  isOpen={decision === 'require_approval' && riskLevel === 'high'}
  riskLevel="high"
  confidenceScore={0.45}
  onApprove={(reason) => sendApproval(reason)}
  onCancel={() => reject()}
/>
```

Features:
- Warning box explaining risks
- Mandatory reason field (textarea, 1-500 chars)
- Confirmation checkbox ("I understand risks...")
- Force Approve button (enabled only when confirmed + reason provided)
- Audit trail: reason + timestamp saved to confidence_audit_log

## Configuration

### Environment Variables

```bash
# Feature toggle
EVOHIME_CONFIDENCE_GATE_ENABLED=1  # default true

# Thresholds (JSON)
EVOHIME_CONFIDENCE_THRESHOLDS='{"none":{"proceed":0.65,...},...}'

# Minimum history for "reliable" tool success rate
EVOHIME_CONFIDENCE_TOOL_MIN_HISTORY=5  # default 5

# Missing signal threshold
EVOHIME_CONFIDENCE_MISSING_SIGNAL_THRESHOLD=2  # require ask if 2+ missing
```

## Migration Path

1. **Phase 1** (complete): Infrastructure (DAOs, events, API endpoints)
2. **Phase 2** (in progress): Runtime integration (emit confidence before approval points)
3. **Phase 3** (TBD): ReAct loop integration (call compute_confidence before tool execution)
4. **Phase 4** (TBD): UI integration (show modals, handle force-approve)
5. **Phase 5** (TBD): Tuning (adjust weights/thresholds based on usage patterns)

## Disabling

```bash
# Completely disable confidence gate
EVOHIME_CONFIDENCE_GATE_ENABLED=0
```

When disabled: falls back to simple uncertainty-based ask gate (6.20 legacy).

## Testing

Run integration tests:

```bash
cargo test -p evohime-agent-runtime --test confidence_gate_integration
```

Test coverage:
- ✅ High-risk always requires approval when low confidence
- ✅ Missing signals trigger ask
- ✅ Weights normalize to 1.0
- ✅ Reliability penalties applied correctly
- ✅ Risk-aware thresholds honored
- ✅ Breakdown structure correct

## Future Work

- [ ] Persistence of confidence settings to DB (currently env-only)
- [ ] Calibration dashboard (show decision vs outcome over time)
- [ ] Auto-tuning of weights based on feedback
- [ ] Integration into planning phase (confidence-aware plan generation)
- [ ] Multi-device sync of confidence settings (7.99 cloud sync)
- [ ] A/B testing framework for threshold experiments
