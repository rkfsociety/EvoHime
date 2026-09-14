# План 172.3 — IPC/status и updater UI

Статус: этап 3 для [плана 172.0](./172-0-updater-recovery-and-self-healing.md).

## Зависимости

### Блокирующие

- План 172.0 и этапы 172.1–172.2.
- Existing `updater.json` status contract.
- Existing Electron `EvoHimeUpdater.exe`, preload и updater renderer.
- Authenticated desktop IPC правила, если добавляются новые действия UI.

### Опциональные

- Existing support bundle и bounded update-failure issue reporter.
- Accessibility и visual contract `docs/update-window-design.md`.

## Реализация

1. Расширить status projection полями recovery phase, active slot/version,
   fallback availability, automatic repair result, retry count, rollback
   availability и typed `reason_code`. Поля bounded и redacted; raw stderr,
   токены и network body не передаются в renderer.
2. Сохранить существующие `checking`, `available`, `applying`, `ready` и
   `failed`, добавив совместимые recovery states без ложного `ready` при
   неполном self-test или неизвестном outcome.
3. В существующем updater UI показать отдельные состояния: автоматическое
   восстановление, обновление control-plane, обычное обновление модулей,
   успешный rollback и manual recovery. Ошибка не закрывает окно самопроизвольно.
4. Добавить только необходимые действия пользователя: повторить recovery,
   запустить проверку ещё раз, открыть штатную инструкцию ручного recovery и
   закрыть окно. Renderer не выбирает artifact, path, hash, release или
   capability.
5. В обычном shell update indicator показывать, что обновление не завершено и
   что updater сохранил fallback. Не запускать повторно внешний effect по
   одному renderer событию; повтор должен идти через updater journal/idempotency.
6. Синхронно обновить typed tests, UI tests и compatibility projections без
   добавления отдельной renderer authority.

## Критерии выхода

- [ ] UI различает recovery, rollback, ready и manual-recovery.
- [ ] `failed` не преобразуется в `ready` из-за stale status или наличия
  доступных модулей.
- [ ] Повтор действия идемпотентен и journal-aware.
- [ ] Renderer получает только bounded metadata и typed next actions.
- [ ] Visual/accessibility contract сохраняет существующую композицию окна.

## Не входит

Новый Electron application, новый IPC protocol major, raw diagnostic console,
автоматическое подтверждение опасных действий и восстановление workspace/data.
