# Wave 3A: Session Recovery (7.116) — Design Specification

**Date:** 2026-07-26  
**Status:** ✅ Implemented (`7.116`)
**Effort:** 3 weeks  
**Dependencies:** None (parallel with Wave 3B)

---

## Goal

Enhance session history retrieval with pagination, improve WebSocket reconnect logic, and add cursor tracking for efficient message replay.

## Current State

- **WebSocket reconnect:** Exponential backoff (max 10s), tracking `lastSequenceRef.current`
- **Message replay:** `?after_sequence=N` query parameter, loads all events after N in one request
- **History API:** `GET /api/sessions/:id/history?after=N` returns all events, no pagination
- **Limitations:** No pagination for large histories, no cursor tracking, inefficient replay for old sessions

## Requirements

### 1. Pagination API for Session History

**Backend (Rust)**

- Extend `GET /api/sessions/:id/history` with keyset-based pagination
- Query parameters:
  - `after` (optional, i64): Load events after this sequence number (current behavior)
  - `limit` (optional, default 50, max 500): Number of events to return
  - `cursor` (optional, base64-encoded): Opaque cursor for keyset pagination
  - `order` (optional, default "asc"): "asc" (forward) or "desc" (backward)

- Response structure:
  ```json
  {
    "items": [
      {
        "sequence": 42,
        "created_at": "2026-07-26T12:00:00Z",
        "event": { ... }
      }
    ],
    "next_cursor": "eyJzZXE...IjoxMzQ2fQ==",
    "prev_cursor": "eyJzZXE...IjoxMjU0fQ==",
    "has_more": true,
    "total_available": 1250
  }
  ```

- Implementation:
  - Use keyset pagination with `(sequence, created_at)` as keys
  - Cursor encodes `{ seq: i64, created_at: DateTime<Utc> }`
  - Backward pagination via `order=desc` for loading old events
  - `total_available` is best-effort (may be approximate for large sets)

**Storage Layer (storage crate)**

- Add `list_session_events_paginated()` function
- Accepts cursor-based offset + limit + order
- Returns paginated result with next/prev cursors

### 2. Reconnect Logic Hardening

**Frontend (React/TypeScript)**

- Enhance `useWebSocket` hook:
  - Track reconnect state in sessionStorage (seq, timestamp, attempt count)
  - Configurable max retry attempts (default 5 before "failed" state)
  - Jitter in exponential backoff to avoid thundering herd
  - Exponential backoff: `delay = base * (2 ^ min(attempt, 5)) + random(0, jitter)`
    - base = 500ms, jitter = 1000ms, max = 32s (or when max attempts exceeded)
  - Emit reconnect lifecycle events: `reconnect.started`, `reconnect.succeeded`, `reconnect.failed`

- Connection state machine:
  ```
  idle → connecting → connected
                  ↓
           reconnecting ↔ connected
                  ↓
              failed (after N attempts)
  ```

- Persist retry context to sessionStorage:
  ```json
  {
    "sessionId": "...",
    "lastSequence": 42,
    "reconnectAttempt": 3,
    "lastConnectTime": "2026-07-26T12:00:00Z"
  }
  ```

- On page load/tab restore:
  - If sessionStorage has recent context (< 5 min), resume from `lastSequence`
  - Clear context after 30 minutes of disconnection

### 3. Message Replay with Cursor Tracking

**Frontend**

- Store in localStorage (not sessionStorage, survives tab close):
  - `replay_cursor`: Base64-encoded pagination cursor
  - `replay_last_seq`: Last received sequence number
  - `replay_last_recv_time`: Timestamp of last successful event

- Reconnect flow:
  1. On socket close, save `lastSequenceRef.current` and `replay_cursor` to localStorage
  2. On reconnect, try `?after_sequence=X` first (fastest path for recent events)
  3. If server returns no events (connection was stale >24h), use paginated API with cursor
  4. Load events in batches of 100 (configurable) via pagination cursor until reaching latest

- Pagination-based backfill:
  ```typescript
  async function backfillHistory(sessionId: string, cursor?: string): Promise<void> {
    let currentCursor = cursor;
    while (true) {
      const page = await api.getSessionHistoryPage({
        sessionId,
        cursor: currentCursor,
        limit: 100
      });
      for (const item of page.items) {
        onEvent(item.event);
        lastSequenceRef.current = item.sequence;
      }
      if (!page.has_more) break;
      currentCursor = page.next_cursor;
    }
  }
  ```

**Database**

- Session recovery state persisted in localStorage (no schema changes needed)
- Cursor-based pagination is computed on query time (no storage of cursors needed)

### 4. API Changes

**New Endpoint Behavior**

- `GET /api/sessions/:id/history` — enhanced with pagination
- `WS /ws/:session_id?after_sequence=N` — unchanged
- `WS /ws/:session_id?cursor=...` — new optional parameter for cursor-based resume (alternative to after_sequence)

### 5. Backward Compatibility

- Existing `after_sequence` parameter still works
- New parameters (`limit`, `cursor`, `order`) are optional
- Clients using old API continue to work unchanged

---

## Implementation Plan

### Phase 1: Backend Pagination (1 week)

1. `evohime_storage`: Add `list_session_events_paginated()` with cursor logic
2. `sessions_api.rs`: Extend handler to support new query parameters
3. Response struct with cursor encoding
4. Tests: pagination, cursor encoding, forward/backward, edge cases

### Phase 2: Frontend Reconnect & Replay (1.5 weeks)

1. Enhance `useWebSocket` hook:
   - Connection state machine
   - Exponential backoff with jitter
   - Max retry attempts
   - Lifecycle events

2. Add replay logic:
   - localStorage cursor persistence
   - Paginated backfill function
   - Integration with useWebSocket

3. Update app.tsx to wire replay on reconnect

### Phase 3: Testing & Hardening (0.5 week)

1. Manual testing: Long sessions, network interruption, tab restore
2. E2E test: Reconnect scenario
3. Load test: Pagination on large histories (10k+ events)
4. Documentation: Cursor format, retry strategy

---

## Success Criteria

- ✅ Pagination API works with cursor, limit, order parameters
- ✅ Reconnect succeeds within 5 attempts for typical network glitches
- ✅ Message replay via pagination works for sessions >1000 events
- ✅ Tab restore recovers session state within 10s
- ✅ No duplicate events after replay (sequence dedupe)
- ✅ Backward compatibility maintained (old clients still work)
- ✅ Documentation: Cursor format, pagination algorithm, retry strategy

---

## Testing Strategy

### Unit Tests

- Cursor encoding/decoding (symmetric)
- Keyset pagination boundary cases (first page, last page, empty)
- Reconnect state transitions
- Backoff calculation

### Integration Tests

- Pagination on real DB with 1k/10k events
- Reconnect → replay → resume flow
- Order parameter (asc/desc)

### Manual Tests

- Network interruption (close browser tools DevTools)
- Tab suspend/restore
- Long session with many events (>5k)
- Check localStorage/sessionStorage state

---

## Risk Mitigation

- **Risk:** Cursor encoding/decoding bugs lead to infinite loops
  - **Mitigation:** Comprehensive unit tests, boundary tests, max iteration limit in backfill loop

- **Risk:** Reconnect storms under high load
  - **Mitigation:** Jitter in backoff, server-side rate limiting already in place

- **Risk:** Large pagination queries (limit=500) overload server
  - **Mitigation:** Cap limit=500, test with realistic payloads

- **Risk:** Session state desync after replay
  - **Mitigation:** Sequence number dedup on client, no idempotent operations assumed in replay

---

## Notes

- Cursor-based pagination chosen over offset-based for:
  - Stability: Insensitive to concurrent inserts/deletes
  - Efficiency: O(1) seek to cursor position
  - UX: No "page number" confusion, natural forward/backward

- Exponential backoff with jitter prevents thundering herd during mass disconnect
- localStorage chosen for replay cursor (survives tab close); sessionStorage for retry context (tab-scoped)
