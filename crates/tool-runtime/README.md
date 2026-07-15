# tool-runtime

Tool registry and execution runtime.

Implemented tools:

- `filesystem.read`
- `filesystem.write`
- `filesystem.patch`
- `filesystem.search`
- `shell.execute`
- `git.status`, `git.diff`, `git.commit`, `git.pull`, `git.push`

All tools run through the registry and workspace sandbox. Permission checks and approval resumption are the next integration boundary; a tool implementation does not yet mean it is callable from the full LLM tool-calling loop.
