# Frontend Shell Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.

**Goal:** Split `app.tsx` into types/api/lib/panels without UX changes.

**Architecture:** Layered extraction; typed `apiRequest`; panels receive props.

---

- [x] Spec
- [x] `types.ts`, `lib/*`, `api/*`
- [x] Extract first panels (Actions/Plugins/Sites)
- [x] Wire `app.tsx`; `npm run build`; commit
- [x] Migrate remaining `fetch` → typed API
- [x] Extract Files/Editor/Git/Tasks/PR/Scheduled panels
- [x] Settings panel + `useServerEventHandler` / `applyServerEvent`
