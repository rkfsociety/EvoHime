# agent-runtime

Agent orchestration loop.

- `agent_loop.rs` — loads chat history, builds plan steps, executes tools in dependency batches (parallel within a batch), runs a bounded observe/replan cycle, then streams the final LLM response

Stage 6: project-index retrieval, session memory, task-scoped model routes, and plan executor (`plan → execute → observe → replan → respond`, max 3 replan rounds).
