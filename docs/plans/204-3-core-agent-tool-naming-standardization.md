# План 204.3 — `evohime-electron` и `evohime-cli`: UI-проекции, карточки активности и CLI

## Scope
Обновление клиентской части и CLI под новые канонические имена инструментов:
- `desktop/evohime-electron/src/renderer/src/`: `chat-summary.ts`, компоненты карточек активности, иконки/лейблы инструментов, локализация (`i18n.ts`), `hive` dashboard и адаптивная панель каталога;
- `desktop/evohime-electron/src/main/`: `repair-service.ts` и прочие обработчики, ссылающиеся на имена инструментов;
- `crates/evohime-cli`: `windows_watch.rs` и связанные проекции вывода.

## Зависимости
- **Blocking:** [План 204.2](./204-2-core-agent-tool-naming-standardization.md).
- **Optional:** нет.

## Изменяемые контракты
- Проекция событий `tool.started` / `tool.output` в UI (`ActivityLine`, `ChatSummaryPanel`, `TaskTimeline`).
- Форматирование и подписи инструментов на русском и английском языках без упоминания старых имён.

## Recovery и безопасность
- Не влияет на security-контракт IPC (`desktop-ipc`).

## Verification
- `npm run typecheck`, `npm test` в `desktop/evohime-electron`.
- Проверка `evohime-cli` тестов в CI.

## Rollback
- Откат коммита UI/CLI этапа; при необходимости синхронный откат этапов 1–2.
