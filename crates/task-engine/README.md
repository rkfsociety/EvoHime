# task-engine

Task lifecycle helpers over PostgreSQL storage.

The crate covers task start/complete/fail transitions together with cancel, resume, retry, steps, checkpoints, dependency batching, and recovery. The current browser workflow already depends on these lifecycle hooks.
