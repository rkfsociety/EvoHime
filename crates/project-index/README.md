# project-index

Project indexing is part of stage 6 and now provides workspace text search for agent context.

The crate exposes a lightweight on-demand index over the workspace. It searches text files, skips heavy directories like `target` and `node_modules`, and returns ranked snippets for `agent-runtime`.

