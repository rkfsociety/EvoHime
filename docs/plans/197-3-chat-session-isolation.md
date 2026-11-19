# План 197.3 — IPC, Electron per-chat состояние и композер без рестарта Core

Статус: active implementation contract. Этап следует после 197.1 и 197.2.

## Scope и изменяемые контракты
1. **Desktop IPC (`crates/desktop-ipc/proto/evohime.desktop.proto`):**
   - Добавить в `StartTask` опциональные поля: `provider`, `profile_id`,
     `model`, `permission_mode`, `isolation_mode`.
   - Добавить в `ModelCatalogRequest` (и при необходимости `ModelConfig`)
     опциональные `provider` и `profile_id`.
   - Перегенерировать TypeScript-типы протокола и подтвердить
     `npm run check:protocol`.
2. **Electron main (`shell-bridge.ts`, `provider-store.ts`, `chat-store`):**
   - Передавать в Core при запуске через supervisor окружение **все**
     настроенные профили провайдеров (все сохранённые ключи/маршруты), а не
     только один выбранный профиль.
   - Убрать `restartCore()` из `provider.select` (или перевести переключение
     провайдера в чистое состояние чата в UI/main). `restartCore()` остаётся
     только при `provider.save` / `provider.clearKey` (когда меняются сами
     секреты/ключи в DPAPI).
   - Расширить модель чата в `chats.json` опциональными полями сессии:
     `provider`, `profileId`, `model`, `permissionMode`, `isolationMode`.
     Старые чаты без этих полей используют глобальные defaults оболочки.
3. **Electron renderer (`TaskTimeline.tsx`, `ChatProviderPicker.tsx`, `ModelPicker.tsx`):**
   - Привязать `ChatProviderPicker` и `ModelPicker` в композере к настройкам
     текущего `chatId` (вместо глобального переключения всего Core).
   - Переключатель режима прав (`read_only` / `ask` / `full`) и опциональный
     переключатель `worktree`-песочницы также хранить per-chat.
   - При отправке сообщения `core.startTask` передаёт параметры сессии
     текущего чата; в списке чатов сохраняется индикация активных задач по
     каждому чату независимо.
   - При использовании `isolated_worktree` (этап 2) показывать в чате статус
     изолированной ветки/worktree и действия просмотра diff / применения.

## Зависимости
### Блокирующие
Этапы 197.1 и 197.2; `crates/desktop-ipc/proto/evohime.desktop.proto`;
`desktop/evohime-electron/src/main/shell-bridge.ts`;
`desktop/evohime-electron/src/main/provider-store.ts`;
`desktop/evohime-electron/src/renderer/src/TaskTimeline.tsx`.

### Опциональные
Дополнительная визуализация worktree diff может опираться на существующие
компоненты просмотра diff/git в UI.

## Recovery и rollback
- Старые записи `chats.json` без полей сессии читаются без миграции с
  подстановкой текущих значений из `provider.json`.
- Секреты (API-ключи) по-прежнему не попадают в `chats.json`, логи и
  renderer: они остаются только в DPAPI (`provider.json`) и окружении Core.

## Verification
- `npm run check:protocol`, `npm run typecheck`, `npm test` в
  `desktop/evohime-electron`.
- Тесты `shell-bridge` на то, что переключение провайдера/модели в чате не
  вызывает `restartCore()`, а `core.startTask` пробрасывает per-chat поля.
