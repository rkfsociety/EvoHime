# План 07-3 — Action Console и approval projection

## Цель

Сделать подтверждение tool action понятным для пользователя и пригодным для
восстановления после restart, сохранив Core единственным владельцем решения.

## Зависимости

### Блокирующие

- [07-1](07-1-tool-manifest-contract.md);
- текущие approval tokens, exact-call recheck, permission policy,
  cancellation и task timeline;
- [06-3](06-3-workflow-desktop.md) для общей Electron projection модели.

### Опциональные

- [07-2](07-2-toolkit-catalog-lifecycle.md). До его готовности карточка
  показывает builtin/MCP identity без catalog metadata.

## Изменения

1. Добавить bounded Core projection action request:
   request id, task/run/node/tool ids, display name, reason, safe arguments
   preview, affected workspace/resources, side effects, required permission,
   budget impact, created/expiry time и status.
2. Редактируемые пользователем данные ограничить feedback и explicit decision;
   tool args, tool id, manifest version, capability и approval binding нельзя
   менять из renderer.
3. В Electron сделать карточки состояний:
   pending, approved, rejected, expired, cancelled, executing, succeeded,
   failed и policy-denied.
4. Поддержать действия «разрешить один раз», «отклонить», «отменить» и
   «отклонить с пояснением», если они совместимы с существующей policy. Не
   добавлять бессрочное разрешение без отдельного явного policy flow.
5. После reconnect/replay показывать ту же карточку по durable event/request
   identity, не создавая повторный effect.
6. Redact secrets, full prompts, arbitrary paths и raw tool output до IPC;
   подробности доступны только через bounded audit/reference projection.

## Проверки

- protocol generation и serialization tests;
- UI tests на reconnect, replay, expiry, duplicate click и Core unavailable;
- negative tests на изменение args/tool/manifest через IPC;
- approval token one-shot и exact-call mismatch tests;
- real-Core E2E: pending → approve/reject → execute/stop;
- `npm run check:protocol`, `npm run typecheck`, `npm test`.

## Готово, когда

Пользователь понимает, что именно произойдёт, принимает или отклоняет действие,
а повторная доставка UI-команды не приводит к повторному side effect.
