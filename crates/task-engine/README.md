# task-engine

Task lifecycle helpers over PostgreSQL storage.

The crate covers task start/complete/fail transitions together with cancel, resume, retry, steps, checkpoints, dependency batching, and recovery. Status changes go through `transition(task_id, TaskStatus)` (load-by-id + FSM); cancel/retry are multi-step FSM helpers.
