# Demo context

This file is used by the demo vertical slice and smoke tests to demonstrate `filesystem.read`.

- The backend reads this file on demand.
- The response is streamed to the browser through WebSocket events.
- You can safely change the contents when you want to verify file-read behavior.

