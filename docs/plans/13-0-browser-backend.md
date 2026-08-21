# План 13 — Изолированный browser backend

## Цель

Добавить управляемый browser capability без превращения desktop UI в
неограниченный computer-use runtime.

## Что уже есть в checkout

- builtin `browser.open`/`browser.extract` tools;
- persistent task browser session через CDP;
- BrowserAccess permission, timeout, cancellation и approval integration;
- Rust tool registry и event/receipt path.

Текущие инструменты не считаются полноценным закрытием плана: нужен отдельный
bounded BrowserContext lifecycle, typed evidence, повторная redirect/SSRF
проверка и packaging/security gate.

## Границы

Входит: BrowserContext на run, typed navigation/click/type/content/state/tabs,
locator/actionability, DOM/accessibility evidence, egress/SSRF policy,
screenshots/traces/artifacts, crash cleanup и packaging.

Не входит: browser extension, unrestricted desktop control, unrestricted
host/network access, raw credentials или внешний Node runtime в package.

## Зависимости

### Блокирующие

- планы 08–12 для ledger, policy, IPC, memory evidence и evaluation;
- текущий Rust tool runtime, BrowserAccess permission и supervisor lifecycle.

### Опциональные

- внешний browser binary/backend; до отдельного packaging/security решения
  capability остаётся disabled и возвращает typed `backend_unavailable`;
- plan 12 evaluation используется как release gate, а без него выполняются
  локальные browser fixtures.

## Этапы

- [13-1 — BrowserContext и typed actions](13-1-browser-context-contract.md)
- [13-2 — network/evidence policy](13-2-browser-network-evidence.md)
- [13-3 — lifecycle, artifacts и packaging](13-3-browser-lifecycle-packaging.md)
- [13-4 — security acceptance](13-4-browser-acceptance.md)

Порядок: 13-1 → 13-2 → 13-3 → 13-4.

## Готово, когда

Browser capability включается явно и permission-gated, каждый action имеет
provenance/receipt, redirect и egress повторно проверяются, artifacts redacted,
а browser process завершается при cancellation, crash и supervisor recovery.
