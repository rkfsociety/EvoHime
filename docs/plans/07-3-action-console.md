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
  показывает builtin/MCP identity и manifest version без catalog metadata
  (source, license, версия пакета) — поля отсутствуют, а не заполняются
  догадками.

## Что уже есть в коде

- Core уже отдаёт approval-запрос с `toolName`, `permission`, `scope` и
  bounded `ApprovalPreview` (summary, command, cwd, path, details, truncated);
- `desktop/evohime-electron/src/renderer/src/TaskTimeline.tsx` показывает одну
  активную карточку с кнопками «Разрешить»/«Отклонить»;
- решение уходит через IPC `core.resolveApproval` с payload
  `{ approvalId, granted }`; Core проверяет one-shot token и exact-call match.

Нет durable identity запроса, переживающей reconnect/restart: карточка живёт
в состоянии renderer и теряется при перезагрузке. Нет состояний expired,
cancelled, policy-denied и executing, нет отображения budget impact и
affected resources, нет idempotency key при повторной доставке решения (в
отличие от `core.confirmMemory`), нет причины отклонения.

## Изменения

1. Добавить bounded Core projection action request:
   request id, task/run/node/tool ids, display name, reason, safe arguments
   preview, affected workspace/resources, side effects, required permission,
   budget impact, created/expiry time и status.
2. Редактируемые пользователем данные ограничить feedback и explicit decision;
   tool args, tool id, manifest version, capability и approval binding нельзя
   менять из renderer.
3. Расширить IPC-контракт решения: к `{ approvalId, granted }` добавить
   idempotency key, опциональную причину отклонения и команду отмены. Старый
   payload остаётся валидным, повторная доставка того же ключа не создаёт
   второй эффект.
4. В Electron сделать карточки состояний:
   pending, approved, rejected, expired, cancelled, executing, succeeded,
   failed и policy-denied.
5. Поддержать действия «разрешить один раз», «отклонить», «отменить» и
   «отклонить с пояснением», если они совместимы с существующей policy. Не
   добавлять бессрочное разрешение без отдельного явного policy flow.
6. После reconnect/replay показывать ту же карточку по durable event/request
   identity, не создавая повторный effect.
7. Redact secrets, full prompts, arbitrary paths и raw tool output до IPC;
   подробности доступны только через bounded audit/reference projection.

## Проверки

- protocol generation и serialization tests;
- UI tests на reconnect, replay, expiry, duplicate click и Core unavailable;
- negative tests на изменение args/tool/manifest через IPC;
- approval token one-shot, idempotency key и exact-call mismatch tests;
- регрессия на существующую карточку в `TaskTimeline.tsx`: старый сценарий
  «разрешить/отклонить» продолжает работать;
- real-Core E2E: pending → approve/reject → execute/stop;
- `npm run check:protocol`, `npm run typecheck`, `npm test`.

## Готово, когда

Пользователь понимает, что именно произойдёт, принимает или отклоняет действие,
а повторная доставка UI-команды не приводит к повторному side effect.
