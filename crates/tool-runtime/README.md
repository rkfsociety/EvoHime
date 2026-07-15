# tool-runtime

Tool registry and execution runtime.

Implemented tools:

- `filesystem.read`
- `filesystem.write`
- `filesystem.patch`
- `filesystem.search`
- `shell.execute`
- `git.status`, `git.diff`, `git.commit`, `git.pull`, `git.push`

All tools run through the registry and workspace sandbox. Permission checks, approval resumption, and task orchestration now work through the browser runtime; stage 6 will add more tools rather than finish the existing base.
