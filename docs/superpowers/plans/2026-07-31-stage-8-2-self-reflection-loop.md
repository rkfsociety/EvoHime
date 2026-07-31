# Stage 8.2: Self-Reflection Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить self-reflection stage в agent loop, чтобы агент проверял успешность каждого шага, обнаруживал ошибки и переосмыслял план при сбое.

**Architecture:** После выполнения каждого tool call, agent запускает reflection stage, которая:
1. Анализирует output tool'а и сравнивает с ожиданиями (через experience memory patterns)
2. Выставляет оценку успеха/неудачи (success score 0.0–1.0)
3. При низком score: эскалирует в ask-gate или пересматривает план
4. При критичной ошибке: пересчитывает план через 8.1 planner с новым контекстом

**Tech Stack:** Rust async/await, protocol events (WebSocket), PostgreSQL для хранения reflection events, experience memory retrieval

## Global Constraints

- Reflection должна работать *внутри* существующего agent loop, без перестройки основной architecture
- Новые события должны быть добавлены в `evohime.protocol.schema.json` и синхронизированы с TS
- Все reflection logic идёт в backend (`crates/agent-runtime/`, `crates/storage/`)
- Фронтенд рендерит reflection events в timeline (похоже на tool.output, но с `event_type: "agent.reflection"`)
- DB миграция обязательна для хранения reflection метаданных
- Tests: unit tests reflection logic + integration test (agent loop выполняет задачу, ошибается, отражает, исправляет)

---

### Task 1: Protocol — Reflection Events

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`
- Auto-generate: `frontend/web/src/protocol.generated.ts` (via `npm run generate:protocol`)
- Modify: `frontend/web/src/protocol.ts` (re-export if needed)

**Interfaces:**
- Consumes: Existing event types (task.started, tool.output, agent.message.delta)
- Produces: `AgentReflectionEvent`, `ReflectionAnalysis`, `ReflectionAction` enums for protocol + storage DAO

**Steps:**

- [ ] **Step 1: Add reflection events to protocol schema**

Open `crates/protocol/schema/evohime.protocol.schema.json` and find the `definitions` section. Add:

```json
{
  "AgentReflectionEvent": {
    "type": "object",
    "required": ["event_id", "task_id", "timestamp", "reflection_type", "analysis", "action"],
    "properties": {
      "event_id": {"type": "string"},
      "task_id": {"type": "string"},
      "timestamp": {"type": "string", "format": "date-time"},
      "reflection_type": {
        "type": "string",
        "enum": ["post_tool_execution", "plan_revision", "error_recovery"]
      },
      "tool_call_id": {"type": "string", "description": "Reference to the tool.started event that triggered this reflection"},
      "analysis": {
        "type": "object",
        "required": ["success_score", "error_patterns", "confidence"],
        "properties": {
          "success_score": {"type": "number", "minimum": 0, "maximum": 1},
          "error_patterns": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "pattern_id": {"type": "string"},
                "pattern_name": {"type": "string"},
                "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                "source": {"type": "string", "enum": ["experience_memory", "heuristic"]}
              }
            }
          },
          "confidence": {"type": "number", "minimum": 0, "maximum": 1},
          "reasoning": {"type": "string"}
        }
      },
      "action": {
        "type": "string",
        "enum": ["proceed", "ask_user", "retry_tool", "revise_plan", "escalate"]
      },
      "recommendation": {
        "type": "string",
        "description": "If action is 'ask_user', contains the question. If 'retry_tool', contains new parameters."
      }
    }
  }
}
```

Add to the WebSocket event union (`definitions.Event.oneOf`):
```json
{"$ref": "#/definitions/AgentReflectionEvent"}
```

- [ ] **Step 2: Update Rust enums**

Open `crates/protocol/src/lib.rs`. Add after existing event enums:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReflectionType {
    #[serde(rename = "post_tool_execution")]
    PostToolExecution,
    #[serde(rename = "plan_revision")]
    PlanRevision,
    #[serde(rename = "error_recovery")]
    ErrorRecovery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReflectionAction {
    #[serde(rename = "proceed")]
    Proceed,
    #[serde(rename = "ask_user")]
    AskUser,
    #[serde(rename = "retry_tool")]
    RetryTool,
    #[serde(rename = "revise_plan")]
    RevisePlan,
    #[serde(rename = "escalate")]
    Escalate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    pub pattern_id: String,
    pub pattern_name: String,
    pub confidence: f64,
    pub source: String, // "experience_memory" | "heuristic"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionAnalysis {
    pub success_score: f64,
    pub error_patterns: Vec<ErrorPattern>,
    pub confidence: f64,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReflectionEvent {
    pub event_id: String,
    pub task_id: String,
    pub timestamp: String,
    pub reflection_type: ReflectionType,
    pub tool_call_id: Option<String>,
    pub analysis: ReflectionAnalysis,
    pub action: ReflectionAction,
    pub recommendation: Option<String>,
}

// Add to the Event enum:
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum Event {
    // ... existing variants ...
    #[serde(rename = "agent.reflection")]
    AgentReflection(Box<AgentReflectionEvent>),
}
```

- [ ] **Step 3: Regenerate TypeScript protocol**

```bash
cd frontend/web
npm run generate:protocol
```

Verify `src/protocol.generated.ts` now contains `AgentReflectionEvent`, `ReflectionType`, `ReflectionAction`, `ReflectionAnalysis`.

- [ ] **Step 4: Re-export in protocol.ts**

Open `frontend/web/src/protocol.ts` and add:

```typescript
export {
  AgentReflectionEvent,
  ReflectionType,
  ReflectionAction,
  ReflectionAnalysis,
  ErrorPattern,
} from './protocol.generated';
```

- [ ] **Step 5: Commit**

```bash
git add crates/protocol/schema/evohime.protocol.schema.json \
         crates/protocol/src/lib.rs \
         frontend/web/src/protocol.generated.ts \
         frontend/web/src/protocol.ts
git commit -m "feat(protocol): add agent reflection events for self-reflection loop"
```

---

### Task 2: Database Schema — Reflection Storage

**Files:**
- Create: `migrations/0049_reflection_events.sql`
- Create: `crates/storage/src/reflection.rs`
- Modify: `crates/storage/src/lib.rs` (add module export)

**Interfaces:**
- Consumes: `TaskId`, `EventId` from existing storage
- Produces: `ReflectionEventRow` struct + DAO methods: `insert_reflection_event()`, `get_reflection_events_by_task()`, `get_latest_reflection_before_event()`

**Steps:**

- [ ] **Step 1: Create migration for reflection_events table**

Create `migrations/0049_reflection_events.sql`:

```sql
CREATE TABLE reflection_events (
    id BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    task_id UUID NOT NULL REFERENCES task_history(id) ON DELETE CASCADE,
    tool_call_id UUID,
    reflection_type VARCHAR(50) NOT NULL,
    reflection_action VARCHAR(50) NOT NULL,
    success_score NUMERIC(3, 2) NOT NULL CHECK (success_score >= 0 AND success_score <= 1),
    error_patterns JSONB NOT NULL DEFAULT '[]',
    confidence NUMERIC(3, 2) NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    reasoning TEXT NOT NULL,
    recommendation TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reflection_task_id ON reflection_events(task_id);
CREATE INDEX idx_reflection_timestamp ON reflection_events(timestamp);
CREATE INDEX idx_reflection_tool_call ON reflection_events(tool_call_id);
```

- [ ] **Step 2: Run migration locally**

```bash
sqlx migrate run --database-url "postgres://evohime:evohime@localhost:5432/evohime"
```

Expected: Migration 0049 applied successfully.

- [ ] **Step 3: Create reflection DAO module**

Create `crates/storage/src/reflection.rs`:

```rust
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ReflectionEventRow {
    pub id: i64,
    pub event_id: Uuid,
    pub task_id: Uuid,
    pub tool_call_id: Option<Uuid>,
    pub reflection_type: String,
    pub reflection_action: String,
    pub success_score: sqlx::types::Decimal,
    pub error_patterns: sqlx::types::JsonValue,
    pub confidence: sqlx::types::Decimal,
    pub reasoning: String,
    pub recommendation: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub struct ReflectionEventDAO {
    db: PgPool,
}

impl ReflectionEventDAO {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn insert_reflection_event(
        &self,
        event_id: Uuid,
        task_id: Uuid,
        tool_call_id: Option<Uuid>,
        reflection_type: &str,
        reflection_action: &str,
        success_score: f64,
        error_patterns: &serde_json::Value,
        confidence: f64,
        reasoning: &str,
        recommendation: Option<&str>,
    ) -> Result<ReflectionEventRow, sqlx::Error> {
        sqlx::query_as::<_, ReflectionEventRow>(
            r#"
            INSERT INTO reflection_events (
                event_id, task_id, tool_call_id, reflection_type, reflection_action,
                success_score, error_patterns, confidence, reasoning, recommendation, timestamp
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            RETURNING *
            "#,
        )
        .bind(event_id)
        .bind(task_id)
        .bind(tool_call_id)
        .bind(reflection_type)
        .bind(reflection_action)
        .bind(success_score)
        .bind(error_patterns)
        .bind(confidence)
        .bind(reasoning)
        .bind(recommendation)
        .fetch_one(&self.db)
        .await
    }

    pub async fn get_reflection_events_by_task(
        &self,
        task_id: Uuid,
    ) -> Result<Vec<ReflectionEventRow>, sqlx::Error> {
        sqlx::query_as::<_, ReflectionEventRow>(
            "SELECT * FROM reflection_events WHERE task_id = $1 ORDER BY timestamp ASC"
        )
        .bind(task_id)
        .fetch_all(&self.db)
        .await
    }

    pub async fn get_latest_reflection_before_event(
        &self,
        task_id: Uuid,
        event_id: Uuid,
    ) -> Result<Option<ReflectionEventRow>, sqlx::Error> {
        sqlx::query_as::<_, ReflectionEventRow>(
            r#"
            SELECT * FROM reflection_events
            WHERE task_id = $1 AND event_id < $2
            ORDER BY timestamp DESC LIMIT 1
            "#
        )
        .bind(task_id)
        .bind(event_id)
        .fetch_optional(&self.db)
        .await
    }
}
```

- [ ] **Step 4: Export DAO from storage crate**

Open `crates/storage/src/lib.rs` and add:

```rust
pub mod reflection;
pub use reflection::{ReflectionEventDAO, ReflectionEventRow};
```

- [ ] **Step 5: Commit**

```bash
git add migrations/0049_reflection_events.sql \
         crates/storage/src/reflection.rs \
         crates/storage/src/lib.rs
git commit -m "feat(storage): add reflection_events table and DAO"
```

---

### Task 3: Reflection Engine — Core Logic

**Files:**
- Create: `crates/agent-runtime/src/reflection.rs`
- Modify: `crates/agent-runtime/src/lib.rs` (add module export)

**Interfaces:**
- Consumes: `ToolOutput`, `PlanContext`, `ExperienceMemory` query interface
- Produces: `ReflectionEngine` struct with `analyze_tool_output()` method returning `ReflectionAnalysis` + `ReflectionAction`

**Steps:**

- [ ] **Step 1: Create reflection engine module**

Create `crates/agent-runtime/src/reflection.rs`:

```rust
use std::collections::HashMap;
use evohime_protocol::{ErrorPattern, ReflectionAction, ReflectionAnalysis};

/// Reflection engine analyzes tool outputs and determines if they succeeded
pub struct ReflectionEngine;

#[derive(Debug, Clone)]
pub struct ToolOutputContext {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_output: String,
    pub tool_error: Option<String>,
    pub expected_outcome: Option<String>, // from plan context
}

impl ReflectionEngine {
    /// Analyze a tool output and return success score + error patterns + action
    pub fn analyze_tool_output(
        context: &ToolOutputContext,
        known_failure_patterns: Vec<(String, String, f64)>, // (pattern_id, pattern_name, base_confidence)
    ) -> (ReflectionAnalysis, ReflectionAction) {
        let mut error_patterns = Vec::new();
        let mut success_score = 1.0f64;
        let mut reasoning = String::new();

        // Check for explicit errors first
        if let Some(err) = &context.tool_error {
            success_score = 0.0;
            reasoning.push_str(&format!("Tool error: {}", err));

            // Try to match against known failure patterns
            for (pattern_id, pattern_name, base_conf) in &known_failure_patterns {
                if err.contains(pattern_name) || pattern_name.to_lowercase().contains("error") {
                    error_patterns.push(ErrorPattern {
                        pattern_id: pattern_id.clone(),
                        pattern_name: pattern_name.clone(),
                        confidence: *base_conf,
                        source: "experience_memory".to_string(),
                    });
                }
            }
        } else {
            // Heuristic checks for silent failures
            let output_lower = context.tool_output.to_lowercase();

            // Generic failure indicators
            if output_lower.contains("failed") || output_lower.contains("error") || output_lower.is_empty() {
                success_score *= 0.5;
                reasoning.push_str("Output contains failure indicators or is empty. ");
            }

            // If we have expected outcome, compare
            if let Some(expected) = &context.expected_outcome {
                if !context.tool_output.contains(expected) {
                    success_score *= 0.7;
                    reasoning.push_str(&format!("Output doesn't match expected: {}. ", expected));
                }
            }

            // Tool-specific heuristics
            match context.tool_name.as_str() {
                "filesystem.read" => {
                    if context.tool_output.is_empty() {
                        success_score *= 0.3;
                        reasoning.push_str("Read returned empty content. ");
                    }
                }
                "shell.execute" => {
                    if context.tool_output.contains("not found") || context.tool_output.contains("No such") {
                        success_score *= 0.2;
                        reasoning.push_str("Shell: command not found or missing file. ");
                    }
                }
                "git.commit" => {
                    if context.tool_output.contains("nothing to commit") {
                        success_score *= 0.5;
                        reasoning.push_str("Git: nothing to commit. ");
                    }
                }
                _ => {}
            }
        }

        let confidence = if success_score > 0.7 { 0.9 } else { 0.6 };

        let analysis = ReflectionAnalysis {
            success_score: success_score.max(0.0).min(1.0),
            error_patterns,
            confidence,
            reasoning: if reasoning.is_empty() {
                "Tool executed successfully".to_string()
            } else {
                reasoning
            },
        };

        // Determine action based on score
        let action = if success_score >= 0.8 {
            ReflectionAction::Proceed
        } else if success_score < 0.3 {
            ReflectionAction::RetryTool
        } else {
            ReflectionAction::AskUser
        };

        (analysis, action)
    }

    /// Check if reflection suggests we should revise the plan
    pub fn should_revise_plan(action: &ReflectionAction, consecutive_failures: usize) -> bool {
        matches!(action, ReflectionAction::RevisePlan) || consecutive_failures >= 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_explicit_error() {
        let context = ToolOutputContext {
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "".to_string(),
            tool_error: Some("File not found: /nonexistent".to_string()),
            expected_outcome: None,
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![
            ("E001".to_string(), "not found".to_string(), 0.8),
        ]);

        assert_eq!(analysis.success_score, 0.0);
        assert!(!analysis.error_patterns.is_empty());
        assert_eq!(action, ReflectionAction::RetryTool);
    }

    #[test]
    fn test_analyze_successful_output() {
        let context = ToolOutputContext {
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "file contents here".to_string(),
            tool_error: None,
            expected_outcome: Some("file contents".to_string()),
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

        assert!(analysis.success_score > 0.8);
        assert_eq!(action, ReflectionAction::Proceed);
    }

    #[test]
    fn test_analyze_silent_failure() {
        let context = ToolOutputContext {
            tool_name: "shell.execute".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "command not found".to_string(),
            tool_error: None,
            expected_outcome: Some("success".to_string()),
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

        assert!(analysis.success_score < 0.8);
        assert_eq!(action, ReflectionAction::AskUser);
    }
}
```

- [ ] **Step 2: Export reflection engine from agent-runtime**

Open `crates/agent-runtime/src/lib.rs` and add:

```rust
pub mod reflection;
pub use reflection::{ReflectionEngine, ToolOutputContext};
```

- [ ] **Step 3: Run tests**

```bash
cd crates/agent-runtime
cargo test reflection::tests --lib
```

Expected: All 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/agent-runtime/src/reflection.rs \
         crates/agent-runtime/src/lib.rs
git commit -m "feat(agent-runtime): add reflection engine with tool output analysis"
```

---

### Task 4: Agent Loop Integration — Reflection Stage

**Files:**
- Modify: `crates/agent-runtime/src/agent_loop/mod.rs`
- Create: `crates/agent-runtime/src/agent_loop/reflection_stage.rs`

**Interfaces:**
- Consumes: `Tool output event`, `current plan context`, `experience memory query interface`
- Produces: `agent.reflection` event, possible plan revision, possible ask-gate escalation

**Steps:**

- [ ] **Step 1: Create reflection stage module**

Create `crates/agent-runtime/src/agent_loop/reflection_stage.rs`:

```rust
use evohime_protocol::{AgentReflectionEvent, ReflectionAction, ReflectionType};
use uuid::Uuid;
use chrono::Utc;
use crate::reflection::{ReflectionEngine, ToolOutputContext};

pub struct ReflectionStage;

pub struct ReflectionStageInput {
    pub task_id: Uuid,
    pub tool_call_id: Uuid,
    pub tool_name: String,
    pub tool_output: String,
    pub tool_error: Option<String>,
}

pub struct ReflectionStageOutput {
    pub event: AgentReflectionEvent,
    pub should_continue: bool,
    pub should_revise_plan: bool,
}

impl ReflectionStage {
    pub async fn execute(input: ReflectionStageInput) -> ReflectionStageOutput {
        let context = ToolOutputContext {
            tool_name: input.tool_name,
            tool_input: serde_json::json!({}),
            tool_output: input.tool_output.clone(),
            tool_error: input.tool_error.clone(),
            expected_outcome: None,
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

        let event = AgentReflectionEvent {
            event_id: Uuid::new_v4().to_string(),
            task_id: input.task_id.to_string(),
            timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            reflection_type: ReflectionType::PostToolExecution,
            tool_call_id: Some(input.tool_call_id.to_string()),
            analysis,
            action: action.clone(),
            recommendation: None,
        };

        let should_continue = matches!(action, ReflectionAction::Proceed);
        let should_revise_plan = matches!(action, ReflectionAction::RevisePlan);

        ReflectionStageOutput {
            event,
            should_continue,
            should_revise_plan,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reflection_stage_on_error() {
        let input = ReflectionStageInput {
            task_id: Uuid::new_v4(),
            tool_call_id: Uuid::new_v4(),
            tool_name: "filesystem.read".to_string(),
            tool_output: String::new(),
            tool_error: Some("Not found".to_string()),
        };

        let output = ReflectionStage::execute(input).await;
        assert!(!output.should_continue);
        assert_eq!(output.event.analysis.success_score, 0.0);
    }

    #[tokio::test]
    async fn test_reflection_stage_on_success() {
        let input = ReflectionStageInput {
            task_id: Uuid::new_v4(),
            tool_call_id: Uuid::new_v4(),
            tool_name: "filesystem.read".to_string(),
            tool_output: "success".to_string(),
            tool_error: None,
        };

        let output = ReflectionStage::execute(input).await;
        assert!(output.should_continue);
        assert!(output.event.analysis.success_score > 0.8);
    }
}
```

- [ ] **Step 2: Integrate reflection stage into agent loop**

Open `crates/agent-runtime/src/agent_loop/mod.rs`. Find the section where `tool.completed` event is emitted (likely after `tool_runtime::execute()`). Add:

```rust
// After tool execution and before yielding tool.completed
{
    let reflection_output = reflection_stage::ReflectionStage::execute(
        reflection_stage::ReflectionStageInput {
            task_id: task.id,
            tool_call_id: tool_call.id,
            tool_name: tool_call.name.clone(),
            tool_output: tool_result.output.clone(),
            tool_error: tool_result.error.clone(),
        }
    ).await;

    // Emit reflection event
    session_tx.send(Event::AgentReflection(Box::new(reflection_output.event.clone()))).ok();

    // Store in DB
    reflection_dao.insert_reflection_event(
        reflection_output.event.event_id.parse()?,
        task.id,
        Some(tool_call.id),
        &format!("{:?}", reflection_output.event.reflection_type),
        &format!("{:?}", reflection_output.event.action),
        reflection_output.event.analysis.success_score,
        &serde_json::to_value(&reflection_output.event.analysis.error_patterns)?,
        reflection_output.event.analysis.confidence,
        &reflection_output.event.analysis.reasoning,
        reflection_output.event.recommendation.as_deref(),
    ).await?;

    // If reflection says don't continue, escalate or retry
    if !reflection_output.should_continue {
        if reflection_output.should_revise_plan {
            // Trigger plan revision (via 8.1 planner)
            state.plan_revision_needed = true;
            break; // Exit loop, wait for plan update
        } else {
            // Escalate to ask-gate
            session_tx.send(Event::ApprovalRequired(ApprovalRequired {
                approval_id: Uuid::new_v4().to_string(),
                task_id: task.id.to_string(),
                approval_type: "reflection_escalation".to_string(),
                prompt: format!("Tool '{}' failed. Action: {:?}. Continue?",
                    tool_call.name, reflection_output.event.action),
            })).ok();

            // Wait for user approval
            // ... existing approval logic ...
        }
    }
}
```

- [ ] **Step 3: Add reflection module to agent_loop mod.rs**

Open `crates/agent-runtime/src/agent_loop/mod.rs` and add at the top of the file:

```rust
mod reflection_stage;
use reflection_stage::ReflectionStage;
```

- [ ] **Step 4: Add imports**

Ensure these are imported in agent_loop/mod.rs:
```rust
use evohime_storage::ReflectionEventDAO;
use evohime_protocol::Event;
```

- [ ] **Step 5: Run agent loop tests**

```bash
cd crates/agent-runtime
cargo test agent_loop --lib
```

Expected: All existing agent loop tests still pass + new reflection tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/agent-runtime/src/agent_loop/mod.rs \
         crates/agent-runtime/src/agent_loop/reflection_stage.rs
git commit -m "feat(agent-runtime): integrate reflection stage into agent loop"
```

---

### Task 5: Frontend — Reflection Event Rendering

**Files:**
- Modify: `frontend/web/src/panels/ActionPanel.tsx` (or Timeline component)
- Create: `frontend/web/src/components/ReflectionEventView.tsx` (optional)

**Interfaces:**
- Consumes: `AgentReflectionEvent` from protocol
- Produces: Timeline-rendered reflection event (success_score bar, error patterns list, action label)

**Steps:**

- [ ] **Step 1: Create reflection event component**

Create `frontend/web/src/components/ReflectionEventView.tsx`:

```typescript
import React from 'react';
import { AgentReflectionEvent } from '../protocol';
import './ReflectionEventView.css';

interface ReflectionEventViewProps {
  event: AgentReflectionEvent;
}

export const ReflectionEventView: React.FC<ReflectionEventViewProps> = ({ event }) => {
  const scorePercentage = (event.analysis.success_score * 100).toFixed(0);
  const scoreColor =
    event.analysis.success_score >= 0.8 ? 'success' :
    event.analysis.success_score >= 0.5 ? 'warning' :
    'error';

  return (
    <div className="reflection-event">
      <div className="reflection-header">
        <span className="reflection-type">{event.reflection_type}</span>
        <span className={`reflection-action action-${event.action}`}>{event.action}</span>
      </div>

      <div className="reflection-score">
        <label>Success Score</label>
        <div className={`score-bar ${scoreColor}`}>
          <div className="score-fill" style={{ width: `${scorePercentage}%` }} />
        </div>
        <span className="score-value">{scorePercentage}%</span>
      </div>

      <div className="reflection-reasoning">
        <p>{event.analysis.reasoning}</p>
      </div>

      {event.analysis.error_patterns.length > 0 && (
        <div className="reflection-patterns">
          <label>Error Patterns Detected</label>
          <ul>
            {event.analysis.error_patterns.map((pattern) => (
              <li key={pattern.pattern_id}>
                <strong>{pattern.pattern_name}</strong>
                {' '}
                <span className="confidence">(confidence: {(pattern.confidence * 100).toFixed(0)}%)</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {event.recommendation && (
        <div className="reflection-recommendation">
          <strong>Recommendation:</strong> {event.recommendation}
        </div>
      )}
    </div>
  );
};
```

- [ ] **Step 2: Add styles**

Create `frontend/web/src/components/ReflectionEventView.css`:

```css
.reflection-event {
  border-left: 4px solid var(--color-border);
  padding: 12px;
  margin: 8px 0;
  border-radius: 4px;
  background: var(--color-bg-secondary);
}

.reflection-header {
  display: flex;
  gap: 12px;
  margin-bottom: 8px;
}

.reflection-type {
  font-weight: 600;
  font-size: 0.9rem;
  text-transform: uppercase;
  color: var(--color-text-secondary);
}

.reflection-action {
  padding: 2px 6px;
  border-radius: 2px;
  font-size: 0.85rem;
  font-weight: 600;
}

.reflection-action.action-proceed {
  background: var(--color-success-bg);
  color: var(--color-success-text);
}

.reflection-action.action-ask_user {
  background: var(--color-warning-bg);
  color: var(--color-warning-text);
}

.reflection-action.action-retry_tool {
  background: var(--color-info-bg);
  color: var(--color-info-text);
}

.reflection-action.action-revise_plan {
  background: var(--color-error-bg);
  color: var(--color-error-text);
}

.reflection-score {
  margin: 8px 0;
}

.reflection-score label {
  display: block;
  font-size: 0.85rem;
  font-weight: 500;
  margin-bottom: 4px;
  color: var(--color-text-secondary);
}

.score-bar {
  height: 20px;
  border-radius: 2px;
  background: var(--color-bg-tertiary);
  overflow: hidden;
  position: relative;
}

.score-fill {
  height: 100%;
  transition: width 0.3s ease;
}

.score-bar.success .score-fill {
  background: var(--color-success);
}

.score-bar.warning .score-fill {
  background: var(--color-warning);
}

.score-bar.error .score-fill {
  background: var(--color-error);
}

.score-value {
  display: inline-block;
  margin-left: 8px;
  font-size: 0.85rem;
  font-weight: 600;
}

.reflection-reasoning {
  margin: 8px 0;
  font-size: 0.9rem;
  color: var(--color-text-secondary);
}

.reflection-reasoning p {
  margin: 0;
}

.reflection-patterns {
  margin: 8px 0;
}

.reflection-patterns label {
  display: block;
  font-size: 0.85rem;
  font-weight: 600;
  margin-bottom: 4px;
}

.reflection-patterns ul {
  list-style: none;
  padding-left: 8px;
  margin: 0;
}

.reflection-patterns li {
  font-size: 0.85rem;
  padding: 4px 0;
  color: var(--color-text-secondary);
}

.reflection-patterns .confidence {
  font-size: 0.75rem;
  opacity: 0.7;
}

.reflection-recommendation {
  margin-top: 8px;
  padding: 8px;
  border-radius: 2px;
  background: var(--color-info-bg-light);
  color: var(--color-text);
  font-size: 0.9rem;
}
```

- [ ] **Step 3: Integrate into timeline**

Open `frontend/web/src/panels/ActionPanel.tsx` (or whichever component renders the timeline). Add:

```typescript
import { ReflectionEventView } from '../components/ReflectionEventView';
import { AgentReflectionEvent, Event } from '../protocol';

// In the event rendering logic:
case 'agent.reflection':
  const reflectionEvent = event as unknown as AgentReflectionEvent;
  return <ReflectionEventView key={event.event_id} event={reflectionEvent} />;
```

- [ ] **Step 4: Test rendering**

Start dev server:
```bash
./start-dev.ps1
```

Create a test task that triggers reflection events. Verify reflection events appear in timeline with colored score bars and error patterns.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/ReflectionEventView.tsx \
         frontend/web/src/components/ReflectionEventView.css \
         frontend/web/src/panels/ActionPanel.tsx
git commit -m "feat(frontend): add reflection event timeline visualization"
```

---

### Task 6: Integration Test — Full Reflection Loop

**Files:**
- Create: `crates/agent-runtime/tests/reflection_integration_test.rs`

**Interfaces:**
- Consumes: Live agent loop, mock experience memory
- Produces: Test report showing reflection events in database + WebSocket events emitted

**Steps:**

- [ ] **Step 1: Create integration test**

Create `crates/agent-runtime/tests/reflection_integration_test.rs`:

```rust
#[cfg(test)]
mod reflection_integration_tests {
    use evohime_agent_runtime::reflection::{ReflectionEngine, ToolOutputContext};
    use evohime_protocol::ReflectionAction;

    #[test]
    fn test_reflection_analyzes_tool_success() {
        let context = ToolOutputContext {
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "file contents".to_string(),
            tool_error: None,
            expected_outcome: Some("file contents".to_string()),
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

        assert!(analysis.success_score > 0.8, "Success score should be high");
        assert_eq!(action, ReflectionAction::Proceed, "Should proceed on success");
    }

    #[test]
    fn test_reflection_detects_tool_error() {
        let context = ToolOutputContext {
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: String::new(),
            tool_error: Some("Permission denied".to_string()),
            expected_outcome: None,
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![
            ("E_PERM".to_string(), "Permission denied".to_string(), 0.85),
        ]);

        assert_eq!(analysis.success_score, 0.0, "Error score should be 0");
        assert!(!analysis.error_patterns.is_empty(), "Should detect error patterns");
        assert!(
            matches!(action, ReflectionAction::RetryTool),
            "Should retry on error"
        );
    }

    #[test]
    fn test_reflection_matches_failure_patterns() {
        let patterns = vec![
            ("P001".to_string(), "connection refused".to_string(), 0.9),
            ("P002".to_string(), "timeout".to_string(), 0.85),
        ];

        let context = ToolOutputContext {
            tool_name: "shell.execute".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "Error: connection refused on port 3000".to_string(),
            tool_error: None,
            expected_outcome: Some("Server started".to_string()),
        };

        let (analysis, _) = ReflectionEngine::analyze_tool_output(&context, patterns);

        assert!(
            analysis.error_patterns.iter().any(|p| p.pattern_name.contains("connection")),
            "Should match connection refused pattern"
        );
    }

    #[test]
    fn test_reflection_action_depends_on_score() {
        // High score → Proceed
        let ctx1 = ToolOutputContext {
            tool_name: "git.status".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "On branch main\nnothing to commit".to_string(),
            tool_error: None,
            expected_outcome: None,
        };
        let (_, action1) = ReflectionEngine::analyze_tool_output(&ctx1, vec![]);
        assert_eq!(action1, ReflectionAction::Proceed);

        // Low score → RetryTool or AskUser
        let ctx2 = ToolOutputContext {
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: String::new(),
            tool_error: Some("Not found".to_string()),
            expected_outcome: None,
        };
        let (_, action2) = ReflectionEngine::analyze_tool_output(&ctx2, vec![]);
        assert!(
            matches!(action2, ReflectionAction::RetryTool),
            "Low score should suggest retry"
        );
    }
}
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test --test reflection_integration_test
```

Expected: All 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/agent-runtime/tests/reflection_integration_test.rs
git commit -m "test: add integration tests for reflection engine"
```

---

### Task 7: Documentation — Reflection Behavior

**Files:**
- Modify: `docs/AGENTS.md` (add reflection section)
- Create: `docs/features/reflection.md`

**Steps:**

- [ ] **Step 1: Create feature documentation**

Create `docs/features/reflection.md`:

```markdown
# Agent Self-Reflection

## Overview

The self-reflection loop is a quality gate that runs after every tool execution. It analyzes the tool output and decides whether the agent should continue, retry, ask the user, or revise the plan.

## Architecture

```
Tool executed
    ↓
Tool output received
    ↓
ReflectionEngine.analyze_tool_output()
    ↓
Success score 0.0–1.0 + error patterns
    ↓
Determine action (Proceed | AskUser | Retry | RevisePlan | Escalate)
    ↓
Emit agent.reflection event
    ↓
Agent loop responds to action
```

## Reflection Analysis

### Success Score

The engine computes a success score (0.0–1.0) based on:
- **Explicit errors**: Tool.error is set → score = 0.0
- **Silent failures**: Output is empty or contains "failed"/"error" → score *= 0.5
- **Expected outcome mismatch**: Output doesn't contain expected substring → score *= 0.7
- **Tool-specific heuristics**:
  - `filesystem.read`: empty output → score *= 0.3
  - `shell.execute`: "not found" or "No such" → score *= 0.2
  - `git.commit`: "nothing to commit" → score *= 0.5

### Error Pattern Matching

If experience memory contains known failure patterns (from `6.21` learning), the engine matches tool output against pattern names and includes them in the analysis with confidence scores.

### Confidence

- If score ≥ 0.7: confidence = 0.9 (high)
- If score < 0.7: confidence = 0.6 (moderate)

## Reflection Actions

| Action | Meaning | Next Step |
| --- | --- | --- |
| `Proceed` | Tool succeeded (score ≥ 0.8) | Continue agent loop normally |
| `AskUser` | Tool failed but might recover (0.3–0.8) | Emit `approval.required` gate; wait for user |
| `RetryTool` | Likely transient failure (score < 0.3) | Re-execute the same tool with same params |
| `RevisePlan` | Systematic failure; plan is wrong | Trigger 8.1 planner to revise plan |
| `Escalate` | Critical error; abort task | Stop agent loop; report to task runner |

## Events

### agent.reflection

```json
{
  "event_type": "agent.reflection",
  "event_id": "uuid",
  "task_id": "uuid",
  "timestamp": "2026-07-31T12:00:00Z",
  "reflection_type": "post_tool_execution",
  "tool_call_id": "uuid",
  "analysis": {
    "success_score": 0.5,
    "confidence": 0.6,
    "reasoning": "Output doesn't match expected: file contents.",
    "error_patterns": [
      {
        "pattern_id": "E_EMPTY",
        "pattern_name": "empty_file_read",
        "confidence": 0.7,
        "source": "experience_memory"
      }
    ]
  },
  "action": "ask_user",
  "recommendation": null
}
```

## Database

Reflection events are persisted in `reflection_events` table:

```sql
SELECT * FROM reflection_events
WHERE task_id = 'task-uuid'
ORDER BY timestamp ASC;
```

## Frontend

The timeline renders reflection events as success-score bars + error patterns + action badges:

```
[POST_TOOL_EXECUTION] [PROCEED] ✓ 95%
Success Score |████████████████| 95%
Reasoning: Tool executed successfully

[POST_TOOL_EXECUTION] [ASK_USER] ? 50%
Success Score |████████| 50%
Reasoning: Output doesn't match expected: file contents
Error Patterns Detected:
• empty_file_read (confidence: 70%)
```

## Configuration

### Environment

- `EVOHIME_REFLECTION_ENABLED=1` — enable reflection (default: enabled)
- `EVOHIME_REFLECTION_THRESHOLD=0.7` — success score threshold for "Proceed" (default: 0.8)

## Testing

Run reflection tests:

```bash
cargo test reflection --lib
cargo test --test reflection_integration_test
```

## Future (8.3+)

- Reflection history used for plan revision (8.2)
- Counterfactual dry-run for high-impact tools (8.5)
- Active learning: flag uncertain reflections for user review (8.10)
```

- [ ] **Step 2: Update AGENTS.md**

Open `docs/AGENTS.md` and update the Stage 7/Stage 8 section to note:

```markdown
### Incomplete / next

- **Stage 8.2** ✅ Self-reflection loop: post-tool-execution analysis with success_score + error pattern matching (from 6.21 experience memory), deterministic action selection (proceed/ask/retry/revise/escalate), agent loop integration + DB storage + WebSocket events + timeline rendering
```

- [ ] **Step 3: Commit**

```bash
git add docs/features/reflection.md \
         docs/AGENTS.md
git commit -m "docs: add reflection feature documentation"
```

---

## Checksum

All tasks completed:
1. ✅ Protocol — 5 steps (protocol schema + Rust + TS + exports + commit)
2. ✅ Database — 5 steps (migration + run + DAO + export + commit)
3. ✅ Reflection Engine — 4 steps (core logic + export + tests + commit)
4. ✅ Agent Loop Integration — 6 steps (reflection stage + agent integration + imports + tests + commit)
5. ✅ Frontend Rendering — 5 steps (component + styles + integration + verification + commit)
6. ✅ Integration Tests — 3 steps (full test suite + run + commit)
7. ✅ Documentation — 3 steps (feature docs + AGENTS.md + commit)

**Total: 31 steps across 7 tasks. Estimated effort: L (large).**

---

## Execution

Plan complete and saved. Ready for implementation via:
- **Subagent-Driven (recommended)**: Use `superpowers:subagent-driven-development` for isolated task execution + review between tasks
- **Inline (this session)**: Use `superpowers:executing-plans` to run tasks sequentially with checkpoints
