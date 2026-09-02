# EvoHime UI theme

## Compact token summary

- Font: `Segoe UI Variable`, `Segoe UI`, system-ui; base 14px; monospace `Cascadia Mono`/Consolas.
- Dark background: `--bg #0d0d11`, raised `#15151b`, sunken `#0a0a0d`; foreground `#e8e8ef`, muted `#9b9baa`, faint `#6d6d7c`.
- Accent: `#8d82e8`, success `#4ec9a4`, warning `#e0b341`, danger `#e26d6d`.
- Borders: white alpha 9% / 16%; hover alpha 6%; active alpha 10%; input alpha 4%.
- Radius: 6px / 10px / 14px. Sidebar 260px, collapsed 58px.
- Light mode is supported through `prefers-color-scheme: light` with background `#f4f4f7` and accent `#6558d6`.

## Source

```css
:root { color-scheme: dark; font-family: 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif; font-size: 14px; --bg:#0d0d11; --bg-raised:#15151b; --bg-sunken:#0a0a0d; --fg:#e8e8ef; --fg-muted:#9b9baa; --fg-faint:#6d6d7c; --accent:#8d82e8; --success:#4ec9a4; --warning:#e0b341; --danger:#e26d6d; --radius-sm:6px; --radius:10px; --radius-lg:14px; --sidebar-width:260px; }
body { margin: 0; height: 100vh; overflow: hidden; background: var(--bg); color: var(--fg); }
.shell { display: grid; grid-template-columns: var(--sidebar-width) 1fr; grid-template-rows: 1fr auto; height: 100vh; }
```
