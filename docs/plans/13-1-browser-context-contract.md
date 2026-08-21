# 13-1 — BrowserContext и typed actions

## Цель

Определить изолированный browser session contract для одного run.

## Изменения

1. Ввести `BrowserContextV1` с run/session identity, target origin allowlist,
   tab IDs, capability snapshot, expiry и cancellation binding.
2. Описать typed actions navigation, click, type, content, state, tabs,
   screenshot и close с bounded inputs/outputs.
3. Использовать locators и actionability checks вместо координатных кликов;
   action target и current page связывать с receipt/event.
4. Разделить read/navigation и side-effecting click/type permissions;
   approval требовать по policy для mutation actions.
5. Запретить context/credentials sharing между runs.

## Проверки

- context isolation и tab lifecycle;
- locator/actionability failure;
- action schema/size/timeout validation;
- permission/approval/cancellation binding;
- replay receipts без повторного side effect.

## Готово, когда

Каждый browser action typed, bounded, связан с run и не может использовать
чужой context или credentials.
