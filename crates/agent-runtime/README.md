# agent-runtime

Agent orchestration loop.

- `agent_loop.rs` — loads chat history, builds plan steps, invokes tools through `model-gateway`, and streams the LLM response

The loop now handles the current stage 5 browser flow, including structured plans, approval-aware tool execution, and recovery hooks. Stage 6 now includes project-index retrieval, session memory context, and task-scoped model routes; it will continue with MCP calls, settings, Python workers, and browser automation.
