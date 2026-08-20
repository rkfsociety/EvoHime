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

Типа `CoreState` в коде нет: разделяются независимые ручки — `EventJournal`
(`journal.clone()`, внутри `Arc<Mutex<LocalDatabase>>`), `Arc<ToolRegistry>` и
registry по образцу `RoutingApprovalRegistry`.

Сверено с кодом, потому что три пункта этапа на этом стоят:

- **Job Object живёт ровно одно поколение Core.** `JobObject::create()`
  вызывается **внутри** цикла рестартов (`windows_supervisor.rs:566`), а сам
  job создан с `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`:202`) и убивает всех
  своих детей в `Drop` (`:251`). Значит формулировка ранней редакции — «ещё
  один ребёнок в том же Job Object» — прямо противоречит требованию «падение
  Core не убивает листенер»: на каждом рестарте Core листенер умирал бы вместе
  с job.
- Рабочий образец второго ребёнка уже есть: `LocalAdapterProcess::spawn_with_limits`
  (`local_provider.rs:125`) создаёт **собственный** job через
  `JobObject::create_with_limits(memory, cpu)` и держит его в поле `_job`
  рядом с `Child`. Образец второго жизненного цикла рядом с основным циклом —
  `run_supervisor_command_channel`, запущенный отдельным `tokio::spawn`
  (`windows_supervisor.rs:501`).
- `JobObject::create_with_limits` и `assign` — `pub(crate)`, поэтому спавн
  листенера обязан жить внутри `evohime-supervisor`.
- Супервизор завершает `run()` при успешном выходе Core или исчерпании
  restart budget (`:605`); тогда job листенера закрывается вместе с процессом
  супервизора и листенер гарантированно не переживает своего надзирателя.
- **`IpcBridge` клонируемым делать не нужно.** `run_windows_pipe` первым
  действием сама заворачивает мост в `Arc` (`pipe_server.rs:83`). Достаточно
  сменить сигнатуру на `Arc<IpcBridge>` и передать один и тот же `Arc` в оба
  сервера. Сегодня функция берёт мост по значению и `await`-ится в конце
  `main.rs:210`, поэтому перевод на две `tokio::spawn`-задачи с ожиданием
  обеих остаётся в силе.
- Цикл `run_windows_pipe` обслуживает **одно соединение за раз**
  (`pipe_server.rs:99`: `loop { create pipe; server.connect().await; …}`),
  поэтому постоянно подключённый листенер занял бы единственный слот и отрезал
  бы shell. Отсюда — второй сервер, а не второй клиент. ACL и nonce-handshake
  переиспользуются как есть (`PipeSecurity::owner_only`).
- `ALLOWED_CLIENT_ROLES` (`crates/desktop-ipc/src/session.rs:35`) объявлен как
  `[&str; 2]` и проверяется в `session.rs:377`; добавление роли `listener`
  меняет и размер массива, и аутентификационный тест — это отдельная правка
  перед всем остальным.
- `MAX_FRAME_BYTES = 4 MiB` (`crates/desktop-ipc/src/lib.rs:9`).
- Пути журналов, которые понадобятся privacy-тесту: Core пишет в
  `<data_dir>/logs/core.jsonl`, супервизор — в
  `%LOCALAPPDATA%\EvoHime\logs\supervisor.jsonl`
  (`windows_supervisor.rs:108`). Никакого `%TEMP%`-фолбэка у логгеров **нет**
  (вопреки ранней редакции: путь `%TEMP%\evohime-log-<pid>.jsonl` встречается
  только в юнит-тесте `logging.rs:54`).

## Содержание

- `crates/evohime-listener-audio`:
  - захват через `cpal` (WASAPI shared, event-driven) — shared mode не отбирает
    устройство у Zoom и Discord;
  - кольцевой буфер в `VirtualLock`-страницах, чтобы снизить риск попадания PCM
    в pagefile. Это best-effort мера, а не доказательство абсолютного
    отсутствия аудио на диске; тестовый инвариант формулируется как отсутствие
    файлового I/O и отключённые WER-дампы;
  - фиксированный полифазный дециматор 48/32→16 кГц (48→16 — ровно /3) и
    `rubato` для нестандартных частот;
  - двухступенчатый VAD: энергетический (RMS + zero-crossing, адаптивный шумовой
    пол) всегда, Silero через `ort` с feature `load-dynamic`, если рядом лежит
    `onnxruntime.dll`;
  - автомат сегментации по лимитам 04.1: вход в речь по трём voiced-кадрам,
    pre-roll 300 мс, выход по 700 мс тишины, минимум 400 мс, потолок 20 с.
    Длинная речь разбивается на последовательные сегменты с `continued`; мост
    в память объединяет их в один кандидат по `episode_id`;
  - крейт не имеет файлового I/O вообще.
- `crates/evohime-listener` (bin): цикл жизни, клиент пайпа с реконнектом и
  backoff, применение политики, опрос активного окна раз в 500 мс
  (`GetForegroundWindow` + `QueryFullProcessImageNameW`), стоп-слово, трейт
  `SpeechEngine` с `FixtureEngine` и `NullEngine`. Собственный файл журнала
  (`<data_dir>/logs/listener.jsonl`) объявляется здесь — на него ссылается
  allow-list privacy-теста ниже.
- `crates/evohime-listener-ipc`: `evohime.listener.proto` (`Hello`/`Handshake`,
  `PolicyUpdate`, `StateChanged`, `UtteranceRecognized`, `EngineStatus`,
  `LocalCommand{ pause | reset_buffers }`) поверх того же framing, что
  desktop-ipc. Лимит кадра — собственный, 256 KiB: у desktop-ipc
  `MAX_FRAME_BYTES = 4 MiB`, и листенеру такой запас не нужен — распознанное
  высказывание на порядки меньше, а узкий потолок ограничивает ущерб от
  скомпрометированного клиента.
- Core: второй pipe-сервер на `<pipe_name>-listener`, роль `listener` в
  `ALLOWED_CLIENT_ROLES`, приём высказываний, выдача политики, проверка
  `MicrophoneListen` перед выдачей разрешения на захват. Оба сервера получают
  один и тот же `Arc<IpcBridge>`.
- `reset_buffers` в `evohime-listener-ipc` сбрасывает только кольцевой буфер, VAD и
  незавершённый сегмент. Удаление транскриптов и производных кандидатов —
  исключительно команда Core/desktop-ipc `ForgetAmbientWindow`.
- При невозможности открыть выбранное устройство листенер
  переходит в `DeviceConflict` с причиной для UI; список устройств и смена
  устройства не требуют перезапуска приложения.
- При уходе Windows в сон захват закрывается и состояние становится
  `PausedByPolicy`; после пробуждения листенер повторно проверяет capability,
  устройство и политику, затем восстанавливает прежнее состояние только если
  это разрешено пользователем.
- Стоп-слово определяется отдельным лёгким детектором параллельно с STT и
  закрывает поток захвата до отправки текущего текста в Core.
- Супервизор: листенер спавнится **не** в job Core, а получает собственный
  job через `create_with_limits` (память и CPU-потолок пригодятся, когда 04.4
  подключит STT), который хранится рядом с `Child` — ровно как в
  `LocalAdapterProcess`. Надзор за листенером живёт в отдельной
  `tokio::spawn`-задаче с собственным restart budget, по образцу
  `run_supervisor_command_channel`; основной цикл продолжает владеть только
  жизненным циклом Core. Следствия, которые и требовались: падение листенера
  не влияет на решение по Core; рестарт Core не трогает листенер, а тот
  переподключается по своему backoff; выход самого супервизора закрывает job
  листенера и гарантированно не оставляет сироту.
- Упаковка: `-p evohime-listener` добавляется в `$cargoArguments`
  (`scripts/build-windows-native.ps1:31`), `evohime-listener.exe` — в
  `$requiredNative` (там же, `:56`), и `listener` — в карту компонентов
  `New-NativePackageManifest` (`scripts/native-package.ps1:19`), иначе бинарь
  окажется в пакете, но не в манифесте, который проверяет
  `scripts/native-package.tests.ps1`. Правок инсталлятора не требуется:
  `installer/EvoHime.iss:43` копирует каталог целиком (`{#SourceDir}\*`).

## Приватность

- аудио-крейт не содержит файлового I/O; проверяется тестом-сканером и
  runtime-тестом. Это не исключает pagefile, который не контролируется
  приложением;
- WER-дампы для процесса отключены, буферы зануляются при паузе и при
  завершении высказывания;
- пауза, тихие часы, чёрный список и стоп-слово **закрывают поток захвата**,
  а не отбрасывают кадры;
- при недоступном Core листенер уходит в `PausedByPolicy`, а не слушает
  «на всякий случай».

## Файлы

- создать: `crates/evohime-listener-audio/**`, `crates/evohime-listener/**`,
  `crates/evohime-listener-ipc/**`, `scripts/ambient-privacy.tests.ps1`;
- изменить: `Cargo.toml` (workspace members) и `Cargo.lock` (в том же
  коммите: CI строит с `--locked`), `.github/workflows/windows.yml` (новые
  пакеты в `cargo test`, шаг для privacy-гейта),
  `crates/desktop-ipc/src/session.rs`,
  `crates/evohime-core/src/pipe_server.rs` (сигнатура `Arc<IpcBridge>`),
  `crates/evohime-core/src/main.rs` (две `tokio::spawn`-задачи),
  `crates/evohime-supervisor/src/windows_supervisor.rs` (надзор за листенером
  отдельной задачей и отдельным job),
  `scripts/build-windows-native.ps1`, `scripts/native-package.ps1`.

## Проверки

- ресемплинг 48→16 совпадает с эталоном байт в байт;
- границы высказываний на фикстурных WAV совпадают с ожидаемыми ±1 кадр;
- `MicrophoneListen = deny` — устройство не открывается;
- совпадение чёрного списка закрывает поток, снятие совпадения открывает;
- стоп-слово даёт паузу, и текущий текст не уходит в Core;
- рестарт Core: листенер переподключается и не теряет состояние паузы. Тест
  обязан проверять именно **выживание процесса** листенера через рестарт Core
  — это прямая проверка того, что job у него собственный, а не общий с Core;
- `listener_audio_has_no_filesystem_io` (скан исходников крейта на `std::fs`,
  `File::`, `OpenOptions`, `tempfile`) и `listener_writes_nothing_but_expected_files`
  (прогон с водяным знаком в PCM, затем обход temp, tools dir и data dir)
  проходят. Второй тест формулируется как **allow-list ожидаемых файлов, а не
  как «ничего не создано»**: листенер законно пишет собственный
  `<data_dir>/logs/listener.jsonl`, Core — `logs/core.jsonl`, супервизор —
  `%LOCALAPPDATA%\EvoHime\logs\supervisor.jsonl`. Тест проверяет, что за
  пределами этого allow-list не появилось файлов и что ни один файл из
  allow-list не содержит водяного знака PCM. Оба теста гоняются гейтом
  `scripts/ambient-privacy.tests.ps1`, который получает собственный шаг в job
  `rust-native` — иначе он не выполняется в CI вовсе.

## Критерии готовности

- листенер запускается супервизором, переживает рестарт Core и соблюдает
  политику;
- на фикстурах сегментация детерминирована;
- приложение не выполняет файловый I/O для сырого аудио, и это доказано двумя
  тестами; pagefile/crash-dump риск не объявляется полностью устранимым.
