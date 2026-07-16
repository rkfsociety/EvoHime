# Agent Logo / Mascot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Встроить гибридный маскот EvoHime (SVG-марка + растр-портрет) во все бренд-слоты UI: favicon, шапка, чат, empty/thinking, sidebar, Settings, Scheduled.

**Architecture:** Статика в `frontend/web/public/brand/` + три презентационных компонента (`AgentMark`, `AgentAvatar`, `AgentBrand`). UI только рендерит ассеты; backend/protocol не трогаем. Портрет выбирается пользователем из 2–3 сгенерированных вариантов перед фиксацией файлов.

**Tech Stack:** React 18, TypeScript, Vite, CSS в `styles.css`, GenerateImage для портрета, ручной SVG для марки.

## Global Constraints

- Frontend only — no backend / protocol / `protocol.generated.ts` changes.
- Hybrid: SVG mark for system/small slots; raster portrait for agent face only.
- Palette: silver hair `#C8D4E6`, teal/blue eyes aligned with `--accent-1` `#3ed7b2` / `--accent-2` `#8fb4ff`.
- Character: tsundere anime hime — cold look, slight smirk.
- Do not put portrait in favicon (16px).
- Minimize diff; reuse existing `.agentBrand` / chat styles.
- After each task: commit. Push only if user asks.
- Verify with `cd frontend/web; npm run build` where noted.

## File Structure

| File | Responsibility |
| --- | --- |
| `frontend/web/public/brand/agent-mark.svg` | Canonical SVG mark |
| `frontend/web/public/favicon.svg` | Tab icon (same mark) |
| `frontend/web/public/favicon.ico` | Legacy fallback (16/32 from mark) |
| `frontend/web/public/brand/agent-avatar-256.webp` | Master portrait |
| `frontend/web/public/brand/agent-avatar-128.webp` | Mid portrait |
| `frontend/web/public/brand/agent-avatar-64.webp` | Small portrait |
| `frontend/web/public/brand/agent-avatar.png` | PNG fallback (optional if webp ok) |
| `frontend/web/src/components/AgentMark.tsx` | Mark `<img>` |
| `frontend/web/src/components/AgentAvatar.tsx` | Portrait `<img>` |
| `frontend/web/src/components/AgentBrand.tsx` | Mark + title |
| `frontend/web/index.html` | Favicon links |
| `frontend/web/src/styles.css` | Brand/avatar/layout classes |
| `frontend/web/src/app.tsx` | Wire top bar, chat, project chip, settings |
| `frontend/web/src/panels/ScheduledPanel.tsx` | Mark in hero |

---

### Task 1: SVG mark + favicon

**Files:**
- Create: `frontend/web/public/brand/agent-mark.svg`
- Create: `frontend/web/public/favicon.svg`
- Create: `frontend/web/public/favicon.ico` (or generate from SVG/PNG)
- Modify: `frontend/web/index.html`

**Interfaces:**
- Produces: `/brand/agent-mark.svg`, `/favicon.svg`, `/favicon.ico` served by Vite from `public/`

- [ ] **Step 1: Create the SVG mark** at `frontend/web/public/brand/agent-mark.svg` with this content (front-facing simplified face, silver + teal, no letters):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" role="img" aria-label="EvoHime">
  <rect width="32" height="32" rx="8" fill="#0b111f"/>
  <!-- hair silhouette -->
  <path fill="#C8D4E6" d="M6 14c0-7 4.5-11 10-11s10 4 10 11v6c-1.5 1-4 2-10 2s-8.5-1-10-2v-6z"/>
  <path fill="#9AABC2" d="M8 12c1-5 3.5-8 8-8 1.2 0 2.3.3 3.3.8C17 6 14 9 13 14c-1.5.2-3.5.2-5-0.5V12z"/>
  <!-- face -->
  <ellipse cx="16" cy="17" rx="7" ry="8" fill="#E8EEF7"/>
  <!-- eyes -->
  <ellipse cx="13" cy="16.5" rx="1.4" ry="1.8" fill="#1a2438"/>
  <ellipse cx="19" cy="16.5" rx="1.4" ry="1.8" fill="#1a2438"/>
  <circle cx="13.4" cy="16.2" r="0.55" fill="#3ed7b2"/>
  <circle cx="19.4" cy="16.2" r="0.55" fill="#3ed7b2"/>
  <!-- smirk -->
  <path d="M14.5 21.2c1.2.9 2.8.9 4 0" fill="none" stroke="#6B7C94" stroke-width="1.1" stroke-linecap="round"/>
</svg>
```

- [ ] **Step 2: Copy the same SVG** to `frontend/web/public/favicon.svg` (identical bytes).

- [ ] **Step 3: Create `favicon.ico`**
  - Prefer: if `magick` is on PATH, convert favicon.svg → ico with 16 and 32.
  - Else: write a minimal multi-size ICO from a 32×32 PNG export, OR place a 32×32 PNG at `frontend/web/public/favicon-32.png` and still produce `favicon.ico` via available converter.
  - Must not use the raster portrait as favicon.

- [ ] **Step 4: Wire `frontend/web/index.html`**

```html
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
    <link rel="icon" href="/favicon.ico" sizes="any" />
    <title>EvoHime</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 5: Verify files exist**

Run (PowerShell, repo root):

```powershell
Test-Path frontend/web/public/brand/agent-mark.svg
Test-Path frontend/web/public/favicon.svg
Test-Path frontend/web/public/favicon.ico
```

Expected: all `True`.

- [ ] **Step 6: Commit**

```powershell
git add frontend/web/public/brand/agent-mark.svg frontend/web/public/favicon.svg frontend/web/public/favicon.ico frontend/web/index.html
git commit -m "feat: add EvoHime SVG mark and favicon"
```

---

### Task 2: Portrait assets (user pick gate)

**Files:**
- Create: `frontend/web/public/brand/agent-avatar-256.webp` (and 128/64)
- Create: `frontend/web/public/brand/agent-avatar.png` (optional fallback)
- Temp: keep rejected variants out of git

**Interfaces:**
- Produces: `/brand/agent-avatar-{256,128,64}.webp` for `AgentAvatar`
- **HARD GATE:** do not commit portrait until user picks a variant

- [ ] **Step 1: Generate 3 portrait variants** with GenerateImage (or equivalent), each:
  - Bust/face of tsundere anime girl
  - Silver/white hair, teal/blue eyes
  - Cold look, slight smirk
  - Dark or transparent-friendly background matching shell `#0b111f`
  - Square crop, face centered, usable at 64px

Example prompts (vary slightly per variant):

1. `Square bust portrait, anime girl mascot, silver-white hair, teal eyes, tsundere cold expression with slight smirk, dark navy background #0b111f, clean UI avatar, no text`
2. Same + `slightly shorter hair, sharper smirk`
3. Same + `side-parted silver hair, cooler lighting`

- [ ] **Step 2: STOP and ask the user** which variant (1/2/3). Do not proceed to commit portrait until answer.

- [ ] **Step 3: Save chosen image** as master `frontend/web/public/brand/agent-avatar-256.webp` (convert to webp if tool returns png; if webp conversion unavailable, save png master and also copy to `agent-avatar.png`, then use png paths in Task 3 — update component src accordingly).

- [ ] **Step 4: Produce 128 and 64**
  - Prefer ImageMagick / available resizer.
  - Fallback: copy the 256 file to `agent-avatar-128.webp` and `agent-avatar-64.webp` (same pixels; CSS will scale). Acceptable for v1.

- [ ] **Step 5: Commit only the chosen assets**

```powershell
git add frontend/web/public/brand/agent-avatar-*.webp
# include png fallback only if created
git commit -m "feat: add EvoHime agent portrait assets"
```

---

### Task 3: Brand components + CSS

**Files:**
- Create: `frontend/web/src/components/AgentMark.tsx`
- Create: `frontend/web/src/components/AgentAvatar.tsx`
- Create: `frontend/web/src/components/AgentBrand.tsx`
- Modify: `frontend/web/src/styles.css` (near `.agentBrand` ~line 72)

**Interfaces:**
- Produces:
  - `AgentMark({ size?: "sm" | "md"; className?: string })`
  - `AgentAvatar({ size?: "sm" | "md" | "lg"; className?: string })`
  - `AgentBrand({ title?: string; markSize?: "sm" | "md"; as?: "h1" | "h2" | "div" })`
- Consumes: `/brand/agent-mark.svg`, `/brand/agent-avatar-{64,128,256}.webp`

- [ ] **Step 1: Create `AgentMark.tsx`**

```tsx
type AgentMarkProps = {
  size?: "sm" | "md";
  className?: string;
};

const sizePx: Record<NonNullable<AgentMarkProps["size"]>, number> = {
  sm: 24,
  md: 32,
};

export function AgentMark({ size = "md", className }: AgentMarkProps) {
  const px = sizePx[size];
  return (
    <img
      className={["agentMark", className].filter(Boolean).join(" ")}
      src="/brand/agent-mark.svg"
      width={px}
      height={px}
      alt=""
      aria-hidden="true"
      draggable={false}
    />
  );
}
```

- [ ] **Step 2: Create `AgentAvatar.tsx`**

```tsx
type AgentAvatarProps = {
  size?: "sm" | "md" | "lg";
  className?: string;
};

const sizeSrc: Record<NonNullable<AgentAvatarProps["size"]>, { src: string; px: number }> = {
  sm: { src: "/brand/agent-avatar-64.webp", px: 28 },
  md: { src: "/brand/agent-avatar-128.webp", px: 40 },
  lg: { src: "/brand/agent-avatar-256.webp", px: 96 },
};

export function AgentAvatar({ size = "md", className }: AgentAvatarProps) {
  const { src, px } = sizeSrc[size];
  return (
    <img
      className={["agentAvatar", className].filter(Boolean).join(" ")}
      src={src}
      width={px}
      height={px}
      alt="EvoHime"
      draggable={false}
    />
  );
}
```

If Task 2 shipped PNG only, change extensions to `.png` and point to `agent-avatar.png` (or sized pngs).

- [ ] **Step 3: Create `AgentBrand.tsx`**

```tsx
import { AgentMark } from "./AgentMark";

type AgentBrandProps = {
  title?: string;
  markSize?: "sm" | "md";
  as?: "h1" | "h2" | "div";
};

export function AgentBrand({ title = "EvoHime", markSize = "md", as = "h1" }: AgentBrandProps) {
  const TitleTag = as;
  return (
    <div className="agentBrand">
      <AgentMark size={markSize} />
      <TitleTag>{title}</TitleTag>
    </div>
  );
}
```

- [ ] **Step 4: Update CSS** — replace/extend `.agentBrand` and add avatar/mark rules:

```css
.agentBrand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.agentBrand h1,
.agentBrand h2 {
  margin: 0;
  font-size: clamp(1.1rem, 2vw, 1.55rem);
  line-height: 1;
  max-width: none;
}

.agentMark {
  display: block;
  flex-shrink: 0;
  border-radius: 8px;
}

.agentAvatar {
  display: block;
  flex-shrink: 0;
  border-radius: 12px;
  object-fit: cover;
  border: 1px solid var(--border-0);
  background: rgba(255, 255, 255, 0.04);
}

.line.assistant {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 10px 12px;
  align-items: start;
}

.line.assistant .agentAvatar {
  grid-row: 1 / span 2;
}

.line.assistant strong {
  grid-column: 2;
}

.line.assistant .markdownBody {
  grid-column: 2;
}

.chatWelcome .agentAvatar {
  margin: 0 auto 12px;
}

.chatTraceSummaryTitle {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.projectOption .agentMark,
.scheduledHero .agentMark {
  flex-shrink: 0;
}

.settingsModalHeader .agentBrand {
  gap: 8px;
}
```

Adjust carefully if existing `.line.assistant` already sets layout — merge instead of duplicating conflicting rules. Keep user/tool/system line layouts intact.

- [ ] **Step 5: Build**

```powershell
cd frontend/web; npm run build
```

Expected: `tsc` + vite succeed.

- [ ] **Step 6: Commit**

```powershell
git add frontend/web/src/components/AgentMark.tsx frontend/web/src/components/AgentAvatar.tsx frontend/web/src/components/AgentBrand.tsx frontend/web/src/styles.css
git commit -m "feat: add AgentMark, AgentAvatar, AgentBrand components"
```

---

### Task 4: Wire top bar + Settings + project chip + Scheduled

**Files:**
- Modify: `frontend/web/src/app.tsx` (imports; ~1596 top bar; ~1409 project option; ~1811 settings header)
- Modify: `frontend/web/src/panels/ScheduledPanel.tsx`

**Interfaces:**
- Consumes: `AgentBrand`, `AgentMark` from Task 3

- [ ] **Step 1: Add imports** in `app.tsx`:

```tsx
import { AgentBrand } from "./components/AgentBrand";
import { AgentMark } from "./components/AgentMark";
import { AgentAvatar } from "./components/AgentAvatar";
```

- [ ] **Step 2: Replace top bar brand**

From:

```tsx
<div className="agentBrand">
  <h1>EvoHime</h1>
</div>
```

To:

```tsx
<AgentBrand />
```

- [ ] **Step 3: Project picker EvoHime option** — replace leading `<span>⌂</span>` with `<AgentMark size="sm" />` for the workspace option labeled EvoHime only.

- [ ] **Step 4: Settings header**

From:

```tsx
<div>
  <span className="sidebarFooterLabel">Настройки</span>
  <h2>Параметры EvoHime</h2>
</div>
```

To:

```tsx
<div>
  <span className="sidebarFooterLabel">Настройки</span>
  <AgentBrand title="Параметры EvoHime" as="h2" markSize="sm" />
</div>
```

- [ ] **Step 5: ScheduledPanel hero** — import `AgentMark` and place before the paragraph:

```tsx
import { AgentMark } from "../components/AgentMark";

// inside scheduledHero:
<section className="scheduledHero">
  <h2>Запланированные задачи</h2>
  <p className="scheduledHeroBrand">
    <AgentMark size="sm" />
    <span>Попросите EvoHime планировать задачи, ставить напоминания или отслеживать обновления</span>
  </p>
</section>
```

Add CSS if needed:

```css
.scheduledHeroBrand {
  display: flex;
  align-items: center;
  gap: 10px;
}
```

- [ ] **Step 6: Build + commit**

```powershell
cd frontend/web; npm run build
git add frontend/web/src/app.tsx frontend/web/src/panels/ScheduledPanel.tsx frontend/web/src/styles.css
git commit -m "feat: wire AgentBrand mark into shell brand slots"
```

---

### Task 5: Wire chat avatar + empty + thinking

**Files:**
- Modify: `frontend/web/src/app.tsx` (chat welcome ~1330; assistant lines ~1356; streaming ~1368; `ChatTraceSummary` ~1849)

**Interfaces:**
- Consumes: `AgentAvatar` from Task 3

- [ ] **Step 1: Empty chat welcome** — replace decorative `✦` with large avatar:

```tsx
<div className="chatWelcome">
  <AgentAvatar size="lg" />
  <p className="eyebrow">Новая задача</p>
  <h3>Что будем делать?</h3>
  {/* rest unchanged */}
</div>
```

- [ ] **Step 2: Assistant message rows** — for `line.role === "assistant"` include avatar:

```tsx
<article className={`line ${line.role}`}>
  {line.role === "assistant" ? <AgentAvatar size="sm" /> : null}
  <strong>{translateChatRole(line.role, githubAuth?.login)}</strong>
  {line.role === "assistant" ? <MarkdownMessage text={line.text} /> : <pre>{line.text}</pre>}
</article>
```

- [ ] **Step 3: Streaming assistant bubble** — same avatar prefix:

```tsx
<article className="line assistant streaming">
  <AgentAvatar size="sm" />
  <strong>Ассистент</strong>
  <MarkdownMessage text={stream} />
</article>
```

- [ ] **Step 4: Thinking / trace summary** — in `ChatTraceSummary` summary title, replace or accompany `thinkingOrb` with small avatar when `active`:

```tsx
<span className="chatTraceSummaryTitle">
  {active ? <AgentAvatar size="sm" className="agentAvatarThinking" /> : <span className="thinkingOrb" aria-hidden="true" />}
  {active ? "Модель работает…" : "Ход работы"}
</span>
```

Keep non-active orb as-is (or always show mark — prefer avatar only while active per «лицо агента»).

- [ ] **Step 5: Build + visual smoke**

```powershell
cd frontend/web; npm run build
```

Expected: PASS. Manually open UI (`.\start-dev.ps1` if user wants stack) and check: favicon, top bar, empty chat, one assistant message layout.

- [ ] **Step 6: Commit**

```powershell
git add frontend/web/src/app.tsx frontend/web/src/styles.css
git commit -m "feat: show agent portrait in chat and empty state"
```

---

### Task 6: Acceptance + docs sync

**Files:**
- Modify: `docs/superpowers/specs/2026-07-16-agent-logo-design.md` (status already `approved`; ensure no trailing junk)
- Modify: `docs/current-state.md` only if it lists UI brand state (one short bullet)

**Interfaces:** none new

- [ ] **Step 1: Checklist against spec Acceptance**

1. Favicon from mark — yes  
2. Top bar mark + EvoHime — yes  
3. Assistant + empty/thinking portrait — yes  
4. Project chip / Settings / Scheduled mark — yes  
5. No portrait-as-favicon — yes  
6. No top bar / chat layout regression — verify after `npm run build` and quick UI look

- [ ] **Step 2: Fix any CSS grid issues** on `.line.assistant` if user/tool messages broke; commit fix if needed.

- [ ] **Step 3: Commit doc sync if changed**

```powershell
git add docs/superpowers/specs/2026-07-16-agent-logo-design.md docs/current-state.md
git commit -m "docs: mark agent logo work done in current-state"
```

(Skip empty commit if nothing to change.)

---

## Spec coverage (self-review)

| Spec item | Task |
| --- | --- |
| SVG mark + palette | Task 1 |
| Favicon svg/ico | Task 1 |
| Raster portrait + sizes | Task 2 |
| AgentMark / Avatar / Brand | Task 3 |
| Top bar | Task 4 |
| Settings / project chip / Scheduled | Task 4 |
| Chat messages / empty / thinking | Task 5 |
| Acceptance | Task 6 |
| Non-goals (no landing, no emotion animation, no backend) | respected |

## Placeholder scan

No TBD/TODO left in steps. Portrait **user pick** is an explicit hard gate in Task 2.
