# Extended Checkpoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Richer task checkpoints and resume that skip completed plan steps.

**Architecture:** Merge `state_json` patches; progress from `task_steps`; agent skips planning when plan is restored.

**Tech Stack:** Rust storage/server/agent-runtime, existing jsonb checkpoints.

---

### Task 1

- [x] `merge_checkpoint_state` / `merge_checkpoint` in storage
- [x] Expand `AgentResumeContext` + skip planning/completed steps
- [x] Server: build resume context, approval/restart pause fields, fix overwrite bug
- [x] Tests + docs + commit
