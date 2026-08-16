# Этап 04.5: Контроль, IPC и UI

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.2 — команды читают и удаляют транскрипты; этап 04.3 —
состояние и политика; этап 04.4 — статус движка.

Разблокирует: 04.6 (бейдж источника в очереди памяти), 04.7 (карточки
предложений).

## Что этап отдаёт наружу

Полную пользовательскую поверхность: команды, события, индикатор записи,
глобальная пауза, панель «Слух» и режим capability микрофона.

## Что уже есть в коде

`oneof command` занят по тег 91 включительно. События идут через `event_type`
и `payload` (`EventEnvelope`), поэтому новых сообщений в `oneof event` не
требуется. `tray.ts` уже перехватывает закрытие окна и имеет готовое место для
индикатора. `SafetyPanel` и `OperationsPanel` существуют.

## Содержание

- Proto: additive команды 92–100 — `SetAmbientListening`, `GetAmbientStatus`,
  `ListAmbientEpisodes`, `GetAmbientEpisode`, `DeleteAmbientTranscripts`,
  `ForgetAmbientWindow`, `GetAmbientPolicy`, `SaveAmbientPolicy`,
  `ResolveAmbientProposal`. Теги не переиспользуются и не переставляются;
  `npm run generate:protocol` выполняется в том же коммите, сгенерированные
  файлы руками не правятся.
- События: `ambient.state`, `ambient.engine`, `ambient.transcript` (только
  метаданные, текст отдаётся отдельным запросом), `ambient.retention`.
- `ipc_bridge.rs`: девять веток по образцу `IndexWorkspace` и соответствующие
  `CoreCommand`.
- `shared/api.ts`: девять записей в `RENDERER_COMMANDS`, `CommandPayloads`,
  `CommandResults`; `shell-bridge.ts`: девять `case` в `dispatch()`.
  `preload/index.ts` не меняется — он проксирует весь список.
- `tray.ts`: иконка-вариант и заголовок «Ева слушает» / «Микрофон на паузе»,
  пункт паузы; `globalShortcut` `Ctrl+Alt+M`. Индикатор **fail-visible**: при
  неизвестном состоянии показывается «слушает», а не «выключено».
- Renderer: `ViewId` получает `'listening'`, новый `ListeningPanel.tsx` —
  крупная строка состояния с причиной, переключатель паузы и подсказка про
  хоткей, «забыть последние 5 минут» и «удалить всё», список эпизодов (время,
  длительность речи, число высказываний, состояние извлечения) с раскрытием
  текста **по явному клику**, редактор политики (тихие часы, чёрные списки,
  срок хранения), установка движка распознавания.
- Индикатор записи в шапке `App.tsx`, не скрываемый ни одной вкладкой.
- `SafetyPanel.tsx`: режим capability `microphone_listen` рядом с остальными и
  строка «за последний час: N высказываний, M кандидатов, K предложений».
- Скачивание рантайма движка в Electron main: пиннированный URL, проверка
  SHA-256, approval, прогресс.

## Файлы

- изменить: `crates/desktop-ipc/proto/evohime.desktop.proto`,
  `crates/evohime-core/src/ipc_bridge.rs`, `crates/evohime-core/src/lib.rs`,
  `desktop/evohime-electron/src/main/ipc/generated/protocol.{js,d.ts}`
  (регенерация), `src/shared/api.ts`, `src/main/shell-bridge.ts`,
  `src/main/tray.ts`, `src/main/index.ts`, `src/renderer/src/App.tsx`,
  `src/renderer/src/SafetyPanel.tsx`, `src/renderer/src/styles.css`;
- создать: `src/renderer/src/ListeningPanel.tsx`,
  `desktop/evohime-electron/resources/evohime-agent-listening.ico`,
  `src/main/listener-runtime.ts`.

## Проверки

- `npm run check:protocol` зелёный после регенерации;
- `npm run typecheck` и renderer-тесты покрывают новую панель;
- пауза из трея, из хоткея и из панели приводит к одному состоянию;
- при потере связи с листенером индикатор показывает «слушает»;
- текст высказывания не приходит в списке, только по явному запросу;
- «забыть последние 5 минут» удаляет и обновляет список без перезапуска.

## Критерии готовности

- пользователь всегда видит, слушают ли его, и может остановить это тремя
  способами;
- удаление транскриптов доступно из UI и необратимо;
- контракт протокола не сломан и сгенерированные файлы совпадают.
