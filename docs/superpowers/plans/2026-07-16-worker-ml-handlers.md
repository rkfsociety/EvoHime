# Worker ML Handlers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `text.summarize` and `text.chunk` stdlib handlers with mirrored Rust validation.

**Architecture:** Extend existing Python JobService handlers and Rust `validate_task_payload`; no new crates.

**Tech Stack:** Python stdlib worker, Rust server validation.

## Global Constraints

- Stdlib only in worker.
- Same MAX_TEXT_LENGTH and HTTP lifecycle.
- Empty text is valid.

---

### Task 1: Python handlers + tests

- [x] Failing tests for summarize and chunk
- [x] Implement handlers + validation
- [x] `python -m unittest discover -s workers/python -p "test_*.py"`

### Task 2: Rust schema mirror

- [x] Extend `validate_task_payload` + SUPPORTED_TASKS
- [x] Unit tests
- [x] `cargo test -p evohime-server -- worker::`

### Task 3: Docs

- [x] README, roadmap, current-state
- [x] Mark plan checkboxes done
