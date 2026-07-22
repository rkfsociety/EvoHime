# Protocol Drift Check Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a CI gate that fails when the generated TypeScript protocol types differ from the repository's committed output.

**Architecture:** Extend the existing GitHub Actions workflow with a dedicated Node 22 job. The job installs frontend dependencies for the existing workspace and root dependencies for the repository-level generator, regenerates `frontend/web/src/protocol.generated.ts`, and checks that regeneration produces no diff.

**Tech Stack:** GitHub Actions, Node.js 22, npm, `json-schema-to-typescript`, Git.

## Global Constraints

- Never edit `frontend/web/src/protocol.generated.ts` by hand.
- Keep protocol generation driven by `npm run generate:protocol`.
- Do not change Rust or frontend production code.
- Do not create a repository branch.

---

### Task 1: Add the protocol drift CI gate

**Files:**
- Modify: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: `frontend/web/package-lock.json`, `npm run generate:protocol`, and the committed generated protocol file.
- Produces: A `protocol-drift` CI job that exits non-zero when generated output is stale.

- [x] **Step 1: Add the job after the existing frontend job**

```yaml
  protocol-drift:
    name: Protocol Drift
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: frontend/web
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "22"
          cache: npm
          cache-dependency-path: frontend/web/package-lock.json
      - name: Install dependencies
        run: npm ci
      - name: Regenerate protocol types
        run: npm run generate:protocol
      - name: Check generated protocol types are committed
        working-directory: .
        run: git diff --exit-code -- frontend/web/src/protocol.generated.ts
```

- [x] **Step 2: Run the same generator locally**

Run: `npm ci` from `C:\github\EvoHime\frontend\web`, then `npm ci; npm run generate:protocol` from `C:\github\EvoHime`.
Expected: exit code 0 and no unintended source changes.

- [x] **Step 3: Verify the drift check locally**

Run: `git diff --exit-code -- frontend/web/src/protocol.generated.ts` from `C:\github\EvoHime`.
Expected: exit code 0.

- [x] **Step 4: Run the frontend typecheck**

Run: `npm run typecheck` from `C:\github\EvoHime\frontend\web`.
Expected: exit code 0.

- [x] **Step 5: Commit the related changes**

```powershell
git add .github/workflows/rust.yml docs/superpowers/plans/2026-07-22-protocol-drift-check.md
git commit -m "ci: check generated protocol types for drift"
```
