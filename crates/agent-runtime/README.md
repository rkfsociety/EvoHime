# agent-runtime

Agent orchestration loop.

- `agent_loop.rs` — reads context, calls the current filesystem read path, and streams the LLM via `model-gateway`

The loop is production-shaped but not yet a general LLM tool-calling orchestrator: dynamic tool-call parsing, approval pauses, and multi-step recovery are still being completed.
