# agent-runtime

Agent orchestration loop.

- `agent_loop.rs` — loads chat history, builds plan steps, invokes tools through `model-gateway`, and streams the LLM response

The loop now handles the current stage 5 browser flow, including structured plans, approval-aware tool execution, and recovery hooks. Stage 6 work will extend it with project-index retrieval, MCP calls, memory, and multi-model routing.
