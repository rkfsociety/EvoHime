# План 144.0 — Модульные релизы и выборочное обновление компонентов

Статус: предложено по инициативе пользователя. Issue не привязан; это
implementation contract, функционал этим документом не считается реализованным.

## Цель

Перевести Windows-поставку EvoHime с модели «один полный installer заменяет
весь пакет» на модель единого продуктового релиза с манифестом компонентов.
Компоненты должны собираться независимо, а установленная Ева должна скачивать
и заменять только изменившиеся совместимые компоненты. Первый обязательный
сценарий — обновление только renderer-интерфейса без загрузки Core,
Supervisor и остальных native-бинарников.

Отдельная версия или артефакт компонента не означает отдельный пользовательский
релиз: один release manifest может указывать версии `ui-bundle`, `core` и
`supervisor`, причём в конкретном патче изменится только один из них.

## Фактическая исходная точка

- Пользователь получает один `EvoHime-Setup.exe`; Electron сейчас поставляется
  как `EvoHime.exe` с bundled renderer.
- `evohime-transaction.exe` уже выполняет staging, backup, commit, health
  handshake и rollback, но installer-обновление в итоге защищает полный tree.
- `evohime.manifest.json` перечисляет package-компоненты, а
  `evohime.build.json` связывает сборку с commit/веткой.
- Основной release channel проверяет commit, размер и SHA-256 установщика.
  Electron autoUpdater, Squirrel и второй update channel не добавляются.
- Listener runtime уже имеет отдельную поставку файлов и собственный
  manifest/hash path; новый план не должен дублировать этот механизм.
- Runtime registry из планов 74/106 — это Core-owned декларативные компоненты,
  а не механизм поставки Windows-бинарников; эти понятия не смешиваются.

## Предлагаемая граница поставки

```text
один release manifest
        │
        ├─ shell-host       (Electron main/preload и обязательные ресурсы)
        ├─ ui-bundle        (renderer assets и UI-код)
        ├─ core             (evohime-core.exe и связанные Core-артефакты)
        ├─ supervisor       (lifecycle, Job Object и recovery)
        ├─ workers          (analysis/listener, если выбран независимый rollout)
        └─ data/runtime packs (модели, DLL и крупные ресурсы)
```

`transaction`, `cli` и `verifier` остаются специальными служебными или
явно выбираемыми компонентами. Их нельзя автоматически объявлять независимыми
только потому, что это отдельные `.exe`. Component boundary определяется
контрактом запуска, жизненным циклом, размером и rollback-поведением.

## Этапы

- [Этап 1 — манифест компонентов и совместимость](./144-1-component-manifest-and-compatibility.md)
- [Этап 2 — выборочная транзакция и recovery](./144-2-selective-transaction-and-recovery.md)
- [Этап 3 — build pipeline, Electron host и UI bundle](./144-3-build-pipeline-and-shell-ui-package.md)
- [Этап 4 — verification, release evidence и закрытие](./144-4-verification-release-evidence-and-closure.md)

## Зависимости

### Блокирующие

- Текущий authenticated/replayed desktop IPC v1 и правило, что renderer
  остаётся projection-only, а Core владеет состоянием и policy.
- Существующие `evohime-transaction.exe`, staged install, health marker,
  recovery journal и полный installer fallback.
- Реальные package/launch contracts Supervisor, Core, Electron, listener и
  analysis worker, проверенные перед фиксацией component IDs и путей.
- GitHub Release manifest/hash workflow, native package manifest, Inno Setup и
  Windows CI; новый механизм не должен обходить существующий release channel.

### Опциональные

- Независимый rollout analysis/listener workers после завершения UI-only
  сценария; без него workers остаются частью совместимого native set.
- Отдельные data/model packs; без них крупные runtime-ресурсы продолжают
  обновляться существующим отдельным listener-runtime path.
- Authenticode/code signing и binary delta patches; они остаются отдельными
  решениями и не становятся скрытой зависимостью этого плана.

## Критерии готовности

- [ ] Единый bounded manifest описывает component IDs, версии, commit/hash,
  артефакты, зависимости, restart class и совместимость.
- [ ] UI-only update загружает и применяет только UI artifact, не заменяя Core,
  Supervisor, transaction worker или пользовательские данные.
- [ ] Несовместимый или неполный manifest блокируется до изменения установки,
  а подходящий полный installer остаётся безопасным fallback.
- [ ] Обновления компонентов атомарны для выбранного набора, переживают crash
  и откатываются после ошибки проверки или health handshake.
- [ ] Build/CI может переиспользовать неизменившиеся артефакты и выполнять
  пропорциональные проверки, сохраняя полный release gate для опасных изменений.
- [ ] Один пользовательский release может содержать один или несколько
  изменившихся компонентов; отдельные module releases не требуются.
- [ ] Тесты не используют установленный рабочий клиент: применяются временные
  install/staging каталоги и изолированные fixtures.

## Non-goals

Не входят отдельный процесс для каждого Rust crate, произвольный renderer
plugin/runtime code, второй update channel, Electron autoUpdater/Squirrel,
обход Core policy/approval, UI-owned SQLite или миграции базы данных из UI,
автоматическое обновление установленного клиента в рамках реализации плана,
а также обязательное введение code signing или binary delta transport.
