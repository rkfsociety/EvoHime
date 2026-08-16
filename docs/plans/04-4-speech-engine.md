# Этап 04.4: Движок распознавания

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.3 — движок получает готовые высказывания от сегментатора;
этап 04.2 — распознанное надо куда-то положить.

Разблокирует: 04.5–04.7.

## Что этап отдаёт наружу

Локальное распознавание речи whisper.cpp с честными состояниями доступности и
дедупликацией; транскрипты доходят до хранилища.

## Что уже есть в коде

Только `FixtureEngine` и `NullEngine` из 04.3. Ни DLL, ни модели, ни резолвера
каталога инструментов.

## Содержание

- `WhisperDllEngine`: `libloading` и собственные FFI-биндинги на C API
  whisper.cpp (`whisper_init_from_file_with_params`, `whisper_full`,
  `whisper_full_n_segments`, `whisper_full_get_segment_text`, `whisper_free` и
  ещё несколько). CMake на этапе `cargo build` не нужен — это условие
  работоспособности self-update, который ставит только Git, Node, Rustup и MSVC
  Build Tools.
- Модель по умолчанию — `ggml-small-q5_1` (~490 МБ): для русской речи заметно
  точнее `base`, а при коротких высказываниях через VAD нагрузка приемлема.
- Резолвер каталога инструментов по порядку: `EVOHIME_LISTENER_TOOLS_DIR` →
  `EVOHIME_TOOLS_DIR` → `F:\github\PROG\evohime-listener` →
  `%LOCALAPPDATA%\EvoHime\tools\listener`. Первый существующий каталог с
  валидным манифестом побеждает; недоступный диск — переход к следующему
  кандидату, а не ошибка.

```
<tools>\listener-runtime.json     манифест: версии, имена файлов, sha256, ABI
<tools>\whisper.dll
<tools>\onnxruntime.dll           опционально; нет — работает энергетический VAD
<tools>\silero_vad.onnx           опционально
<tools>\models\ggml-small-q5_1.bin
```

- Несовпадение SHA-256 или ABI — `EngineUnavailable` с кодом, а не загрузка.
- Первая установка: кнопка в UI (этап 04.5) → Electron main качает по
  захардкоженному URL с проверкой SHA-256, как approval-действие. Агент и Core
  ничего не скачивают.
- Дедупликация: `text_hash` (NFKC, lowercase, без пунктуации) в окне 60 с плюс
  near-dup по token-set ratio ≥ 0.9 против пяти предыдущих высказываний.
  Подавленное считается счётчиком, а не пишется.
- Бюджет: измерение RTF; RTF > 0.5 на пяти высказываниях подряд — деградация
  модели и событие `ambient.engine` с причиной.
- Модель открывается read-only mmap; параметры `whisper_full` фиксированы и не
  содержат путей вывода.
- Опциональная cargo-feature `whisper-static` (`whisper-rs`) для dev-сборок;
  в packaging не участвует.

## Файлы

- создать: `crates/evohime-listener/src/engine/{mod.rs,whisper_dll.rs,fixture.rs,null.rs}`,
  `crates/evohime-listener/src/tools_dir.rs`;
- изменить: `crates/evohime-listener/Cargo.toml`,
  `crates/evohime-core/src/lib.rs` (приём и запись транскриптов),
  `docs/architecture.md`.

## Проверки

- отсутствие DLL или модели даёт `engine_unavailable`, а не тишину и не панику;
- битый или подменённый файл по SHA-256 не загружается;
- дедупликация подавляет повтор и считает его счётчиком;
- деградация по RTF срабатывает и откатывается;
- `EVOHIME_LISTENER_ENGINE_E2E=1` прогоняет реальный движок на фикстурах; без
  переменной используется `FixtureEngine`, и CI зелёный без модели — тем же
  приёмом, что `EVOHIME_UPDATE_E2E`.

## Критерии готовности

- транскрипты появляются в хранилище и видны через IPC;
- отсутствие движка — честное состояние, а не маскировка;
- ни один шаг сборки продукта не требует CMake.
