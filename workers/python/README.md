# Python workers

Isolated workers for heavier AI/ML tasks. Planned for stage 6.

## Layout

```text
workers/python/
├── README.md
└── worker.py
```

The Rust server will enqueue jobs and consume structured results over HTTP or a message queue once stage 6 starts.
