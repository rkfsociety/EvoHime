# Demo context

This file is used by the demo vertical slice and smoke tests to demonstrate `filesystem.read`.

- The Rust Core reads this file on demand.
- The response is streamed to the desktop task timeline through IPC events.
- You can safely change the contents when you want to verify file-read behavior.

