# EvoHime — design system

## Product context

EvoHime is a local Windows AI-agent desktop application. The renderer is a
thin presentation layer over typed IPC; screens show Core-confirmed state and
must not invent success, data, or actions. The operations view groups memory,
Pulse, repair, workspace knowledge, refinement, heard suggestions, child
contexts, and explicit user approvals.

## Visual direction

Calm, dense-but-readable desktop tooling. Preserve the graphite dark shell,
violet primary accent, turquoise success, amber warnings, and compact status
pills. Use clear sections, strong hierarchy, generous enough vertical rhythm,
and short explanatory labels. The screen must feel like a reliable control
surface rather than a marketing page. Keep all existing Russian labels and
meaningful states; never hide an error behind decorative styling.

## Tokens

- Font: `Segoe UI Variable`, `Segoe UI`, system-ui; base 14px.
- Monospace: `Cascadia Mono`, Consolas.
- Background: `--bg #0d0d11`; raised `--bg-raised #15151b`; sunken `--bg-sunken #0a0a0d`.
- Foreground: `--fg #e8e8ef`; muted `--fg-muted #9b9baa`; faint `--fg-faint #6d6d7c`.
- Accent: `--accent #8d82e8`; success `--success #4ec9a4`; warning `--warning #e0b341`; danger `--danger #e26d6d`.
- Borders use white alpha 9% / 16%; hover alpha 6%; active alpha 10%.
- Radius: 6px small, 10px default, 14px large.
- Shell: 260px sidebar + flexible content, persistent 28px status bar.

## Component rules

- Cards are raised surfaces with a quiet border, 10–12px radius, and 12–16px padding.
- Status pills are compact, semantic, and always paired with understandable text.
- Primary actions use the violet accent; destructive or irreversible actions retain explicit wording and approval boundaries.
- Long diagnostics, hashes, paths, and error digests use monospace and wrap safely instead of overflowing.
- Responsive behavior collapses multi-column cards to one column below 900px.
- Keep keyboard focus visible and preserve native control semantics.

## Operations view direction

Use a clear page header with title, one-line explanation, and connection state.
Make the first row an at-a-glance status dashboard: repair, memory approval,
memory conflicts, child jobs, Pulse, and tools. Give repair a visible but
controlled warning treatment. Below it, separate workspace knowledge and
action controls from refinement, heard suggestions, memory approvals, conflict
resolution, and child timelines. Prefer compact summary cards and grouped
action rows over an undifferentiated wall of text.
