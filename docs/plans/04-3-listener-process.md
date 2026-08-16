# Этап 04.3: Процесс листенера, захват и сегментация

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.1 — состояния, лимиты и политика; этап 04.2 — Core должен
уметь принять высказывание и отдать политику.

Разблокирует: 04.4–04.7.

## Что этап отдаёт наружу

Работающий `evohime-listener.exe` под супервизором: захват, ресемплинг, VAD,
сегментация, пауза, чёрный список. Движок STT в этом этапе фикстурный.

## Что уже есть в коде

Супервизор спавнит ровно одного ребёнка и завершает цикл после успешного
выхода Core (`windows_supervisor.rs`). Core-пайп обслуживает одно соединение за
раз (`pipe_server.rs`). `ALLOWED_CLIENT_ROLES` (`crates/desktop-ipc/src/session.rs`)
содержит две роли.

## Содержание

- `crates/evohime-listener-audio`:
  - захват через `cpal` (WASAPI shared, event-driven) — shared mode не отбирает
    устройство у Zoom и Discord;
  - кольцевой буфер в `VirtualLock`-страницах, чтобы PCM не уходил в pagefile;
  - фиксированный полифазный дециматор 48/32→16 кГц (48→16 — ровно /3) и
    `rubato` для нестандартных частот;
  - двухступенчатый VAD: энергетический (RMS + zero-crossing, адаптивный шумовой
    пол) всегда, Silero через `ort` с feature `load-dynamic`, если рядом лежит
    `onnxruntime.dll`;
  - автомат сегментации по лимитам 04.1: вход в речь по трём voiced-кадрам,
    pre-roll 300 мс, выход по 700 мс тишины, минимум 400 мс, потолок 20 с с
    флагом `continued`;
  - крейт не имеет файлового I/O вообще.
- `crates/evohime-listener` (bin): цикл жизни, клиент пайпа с реконнектом и
  backoff, применение политики, опрос активного окна раз в 500 мс
  (`GetForegroundWindow` + `QueryFullProcessImageNameW`), стоп-слово, трейт
  `SpeechEngine` с `FixtureEngine` и `NullEngine`.
- `crates/listener-ipc`: `evohime.listener.proto` (`Hello`/`Handshake`,
  `PolicyUpdate`, `StateChanged`, `UtteranceRecognized`, `EngineStatus`,
  `LocalCommand{ pause | forget_window }`) поверх того же framing, что
  desktop-ipc; frame limit 256 KiB.
- Core: второй pipe-сервер на `<pipe_name>-listener`, роль `listener` в
  `ALLOWED_CLIENT_ROLES`, приём высказываний, выдача политики, проверка
  `MicrophoneListen` перед выдачей разрешения на захват.
- Супервизор: второй ребёнок в том же Job Object с собственным restart budget.
  Падение листенера не влияет на решение по Core; падение Core не убивает
  листенер до исчерпания реконнектов.
- Упаковка: `-p evohime-listener` в `scripts/build-windows-native.ps1` и
  `evohime-listener.exe` в `$requiredNative`.

## Приватность

- аудио-крейт не содержит файлового I/O; проверяется тестом-сканером;
- WER-дампы для процесса отключены, буферы зануляются при паузе и при
  завершении высказывания;
- пауза, тихие часы, чёрный список и стоп-слово **закрывают поток захвата**,
  а не отбрасывают кадры;
- при недоступном Core листенер уходит в `PausedByPolicy`, а не слушает
  «на всякий случай».

## Файлы

- создать: `crates/evohime-listener-audio/**`, `crates/evohime-listener/**`,
  `crates/listener-ipc/**`;
- изменить: `Cargo.toml`, `crates/desktop-ipc/src/session.rs`,
  `crates/evohime-core/src/pipe_server.rs`, `crates/evohime-core/src/main.rs`,
  `crates/evohime-supervisor/src/windows_supervisor.rs`,
  `scripts/build-windows-native.ps1`.

## Проверки

- ресемплинг 48→16 совпадает с эталоном байт в байт;
- границы высказываний на фикстурных WAV совпадают с ожидаемыми ±1 кадр;
- `MicrophoneListen = deny` — устройство не открывается;
- совпадение чёрного списка закрывает поток, снятие совпадения открывает;
- стоп-слово даёт паузу, и текущий текст не уходит в Core;
- рестарт Core: листенер переподключается и не теряет состояние паузы;
- `listener_audio_has_no_filesystem_io` (скан исходников крейта на `std::fs`,
  `File::`, `OpenOptions`, `tempfile`) и `listener_writes_nothing_but_expected_files`
  (прогон с водяным знаком в PCM, затем обход temp, tools dir и data dir)
  проходят; оба гоняются гейтом `scripts/ambient-privacy.tests.ps1`.

## Критерии готовности

- листенер запускается супервизором, переживает рестарт Core и соблюдает
  политику;
- на фикстурах сегментация детерминирована;
- сырое аудио не попадает на диск, и это доказано двумя тестами.
