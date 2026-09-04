# План 144.2 — Модульные релизы: выборочная транзакция и recovery

Статус: этап 2 для [плана 144.0](./144-0-modular-release-and-component-update.md).

## Зависимости

### Блокирующие

- Завершённый контракт и compatibility matrix этапа
  [144.1](./144-1-component-manifest-and-compatibility.md).
- `evohime-transaction.exe` с self-copy/re-exec, journal, backup, health
  handshake и восстановлением незавершённой транзакции.
- Windows process/Job Object contracts Supervisor и Electron shutdown/restart.

### Опциональные

- Независимые workers и data packs; при отсутствии отдельной restart/recovery
  стратегии они входят в coordinated component set или full installer.
- Изменения existing listener-runtime apply path; он остаётся отдельным и не
  должен зависеть от desktop component transaction.

## Реализация

1. Расширить transaction model до явно выбранного `component-set` с
   operation ID, manifest hash, pre-state inventory, selected paths, backup
   scope, phase и recovery decision. Сохранить full-tree scope как fallback для
   installer/source rebuild.
2. Реализовать manifest-first download: получить и проверить release manifest,
   выбрать совместимые артефакты, скачать только нужные файлы в bounded staging,
   сверить размер и SHA-256 каждого, затем записать staging manifest последним
   шагом. До полной проверки установка не меняется.
3. Разделить apply strategies:
   - `ui-bundle` — versioned directory/archive с atomic active pointer;
   - обычный native file — replace только выбранного файла после выхода
     удерживающих его процессов;
   - `shell-host`/`supervisor`/`transaction` — controlled restart с внешним
     worker и обязательным health handshake;
   - schema-changing Core — coordinated path с SQLite backup и явным
     forward-recovery, без ложного rollback после необратимой миграции.
4. Для UI-only сценария вынести renderer из неизменяемого bundled `app.asar` в
   проверяемый versioned `ui-bundle` layout. Stable Electron main/preload
   выбирает только активную validated version, загружает её после проверки
   manifest/hash и при ошибке возвращается к предыдущей версии.
5. Сохранить целостность выбранного набора: если apply, launch, authenticated
   Core connection или health marker не подтверждены, rollback возвращает
   прежние выбранные компоненты. Пользовательские данные, secrets, SQLite и
   незаявленные extra files не удаляются.
6. Добавить recovery для power loss, process crash, stale transaction, partial
   copy, locked file, missing artifact, invalid health marker и повторного
   запуска. Unknown outcome остаётся typed failure/recovery-required и не
   запускает blind retry.
7. Добавить безопасный fallback: старый клиент без component manifest или с
   неподдерживаемым component operation использует текущий проверенный полный
   installer path, а не частично применяет неизвестный пакет.

## Критерии выхода

- [ ] UI-only apply меняет только versioned UI path и требует только restart
  shell; hash Core/Supervisor и их файлы остаются неизменными.
- [ ] Component-set apply атомарен относительно выбранного набора и журнал
  позволяет восстановиться после каждой промежуточной фазы.
- [ ] Ошибки до commit не оставляют half-written selected component; rollback
  и recovery подтверждены временными Windows fixtures.
- [ ] Health handshake и protocol/ABI checks выполняются до удаления старого
  набора; старый рабочий набор сохраняется до commit.
- [ ] Full installer/source rebuild и существующий listener-runtime path не
  регрессировали.

## Не входит

Не входят обновление установленного клиентского экземпляра во время разработки,
автономное исправление SQLite без Core migration contract, скрытый restart
работающей сессии и удаление неизвестных файлов из install directory.
