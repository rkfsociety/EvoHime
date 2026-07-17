# project-index

On-demand workspace text search for agent context (stage 6 / P2).

## Features

- Chunk merging of adjacent line hits (stable text for future embeddings)
- Separate ranking weights: path hits, symbol/definition hits, plain content
- Skips binary files, noisy extensions (`.png`, `.min.js`, lockfiles, …), and heavy dirs (`target`, `node_modules`, `.git`, …)

`agent-runtime` calls `ProjectIndex::build_context` when building model prompts.
