# План реализации адаптивного журнала установщика

> **Для агентных исполнителей:** ОБЯЗАТЕЛЬНЫЙ ДОПОЛНИТЕЛЬНЫЙ НАВЫК: выполняйте этот план по задачам с помощью `superpowers:subagent-driven-development` (рекомендуется) или `superpowers:executing-plans`. Шаги используют синтаксис флажков (`- [ ]`).

**Цель:** сделать журнал подробностей установщика постоянно видимым, адаптивным по ширине и высоте, выделяемым и полностью копируемым.

**Архитектура:** `InstallerApp` хранит один канонический `String` и инкрементально дописывает в него события прогресса. Неизменяемое представление `&str` отображается через `TextEdit::multiline` внутри вертикального `ScrollArea` со штатными переносом строк и удержанием нижнего края; явное копирование проходит через небольшой helper на границе clipboard API, тестируемый без системного буфера Windows.

**Стек:** Rust 2021, eframe/egui 0.35, встроенные тесты Rust, Windows PowerShell.

## Глобальные ограничения

- Работать прямо в текущей `main`; не создавать ветку или worktree.
- Задать начальный размер клиентской области через `ViewportBuilder::with_inner_size([900.0, 720.0])`.
- Задать минимальный размер клиентской области через `ViewportBuilder::with_min_inner_size([720.0, 720.0])`.
- Считать эти размеры логическими points egui; физические пиксели определяются масштабом Windows.
- Показывать «Подробности» до, во время и после установки без элемента сворачивания.
- Сохранить все существующие сообщения журнала, их порядок и формат.
- Не менять логику установки, сети, прогресса и обработки ошибок.
- Переносить длинные строки по доступной ширине; не добавлять горизонтальную прокрутку и не обрезать текст.
- Использовать `ScrollArea::stick_to_bottom(true)` для условного слежения за концом журнала.
- Сохранять журнал доступным после завершения установки до закрытия окна.
- Не добавлять зависимости и не изменять сгенерированные файлы.
- Для каждого поведения сначала запускать тест и наблюдать ожидаемое падение.
- После финальной сборки и визуальной проверки удалить workspace `target/`.

---

### Задача 1: Канонический буфер журнала и граница clipboard API

**Файлы:**
- Изменить: `crates/installer/src/main.rs:10-154`
- Изменить: `crates/installer/src/lib.rs`
- Создать: `crates/installer/src/ui.rs`
- Создать: `crates/installer/tests/ui.rs`

**Интерфейсы:**
- Создаёт: `fn append_log_entry(log: &mut String, entry: &str)`
- Создаёт: `fn can_copy_log(log: &str) -> bool`
- Создаёт: `fn copy_log_to_clipboard(ctx: &egui::Context, log: &str)`
- Создаёт: `InstallerApp::log: String`
- Использует: существующие `ProgressEvent::Stage(String)` и `ProgressEvent::Error(String)`

- [x] **Шаг 1: Написать падающие тесты точного инкрементального текста и clipboard output**

Добавить эти integration tests в `crates/installer/tests/ui.rs`. Нейтральное
имя test target обязательно на Windows: unit-test harness с именем
`evohime-setup` или `evohime-installer` останавливается до запуска тестов с
`os error 740`.

```rust
use evohime_installer::ui::{
    append_log_entry, can_copy_log, copy_log_to_clipboard,
};

fn copied_text(output: &eframe::egui::FullOutput) -> Option<&str> {
    output
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            eframe::egui::OutputCommand::CopyText(text) => Some(text.as_str()),
            _ => None,
        })
}

#[test]
fn appends_progress_entries_without_changing_their_text() {
    let mut log = String::new();

    append_log_entry(&mut log, "Проверка свободного места на диске...");
    append_log_entry(&mut log, "Скачивание server.zip...");
    append_log_entry(&mut log, "Ошибка: unexpected HTTP status 416");

    assert_eq!(
        log,
        "Проверка свободного места на диске...\n\
         Скачивание server.zip...\n\
         Ошибка: unexpected HTTP status 416"
    );
}

#[test]
fn copy_action_emits_the_exact_canonical_log_text() {
    let log = "Первая строка\nОшибка: вторая строка";
    let ctx = eframe::egui::Context::default();

    let output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        copy_log_to_clipboard(ui.ctx(), log);
    });

    assert_eq!(copied_text(&output), Some(log));
    assert!(can_copy_log(log));
    assert!(!can_copy_log(""));
}
```

- [x] **Шаг 2: Запустить тесты и подтвердить состояние RED**

Выполнить:

```powershell
cargo test -p evohime-installer --test ui appends_progress_entries_without_changing_their_text
cargo test -p evohime-installer --test ui copy_action_emits_the_exact_canonical_log_text
```

Ожидается: компиляция падает из-за отсутствующих `append_log_entry`, `copy_log_to_clipboard` и `can_copy_log`. Причиной должны быть отсутствующие production-функции, а не синтаксис теста или зависимости.

- [x] **Шаг 3: Реализовать минимальные helper-функции канонического буфера**

Объявить `pub mod ui;` в `crates/installer/src/lib.rs` и добавить эти
функции в `crates/installer/src/ui.rs`:

```rust
pub fn append_log_entry(log: &mut String, entry: &str) {
    if !log.is_empty() {
        log.push('\n');
    }
    log.push_str(entry);
}

pub fn can_copy_log(log: &str) -> bool {
    !log.is_empty()
}

pub fn copy_log_to_clipboard(ctx: &egui::Context, log: &str) {
    if can_copy_log(log) {
        ctx.copy_text(log.to_owned());
    }
}
```

Заменить поле и конструктор приложения:

```rust
struct InstallerApp {
    rx: Option<mpsc::Receiver<ProgressEvent>>,
    current_stage: String,
    log: String,
    steps_done: usize,
    finished: bool,
    failed: bool,
    started: bool,
}
```

```rust
log: String::new(),
```

Заменить два существующих добавления записей, не меняя их текст:

```rust
ProgressEvent::Stage(msg) => {
    self.steps_done += 1;
    self.current_stage = msg.clone();
    append_log_entry(&mut self.log, &msg);
}
ProgressEvent::Error(msg) => {
    self.current_stage = "Установка прервана из-за ошибки.".to_string();
    append_log_entry(&mut self.log, &format!("Ошибка: {msg}"));
    self.failed = true;
}
```

- [x] **Шаг 4: Запустить точечные и полные тесты crate и подтвердить GREEN**

Выполнить:

```powershell
cargo test -p evohime-installer --test ui
cargo test -p evohime-installer --test icacls_windows
cargo check -p evohime-installer
```

Ожидается: оба UI-теста и оба существующих ACL-теста проходят, а весь crate
успешно компилируется. Полный `cargo test -p evohime-installer` не
использовать: Windows требует elevation для автоматически названных unit-test
harness установщика до запуска тестов.

- [x] **Шаг 5: Закоммитить каноническое поведение журнала**

```powershell
git add -- crates/installer/src/main.rs crates/installer/src/lib.rs crates/installer/src/ui.rs crates/installer/tests/ui.rs docs/superpowers/plans/2026-07-28-installer-details-layout.md
git commit -m "feat(installer): keep canonical copyable log text"
```

---

### Задача 2: Постоянный адаптивный интерфейс подробностей

**Файлы:**
- Изменить: `crates/installer/src/main.rs:50-245`
- Изменить: `crates/installer/src/ui.rs`
- Тестировать: `crates/installer/tests/ui.rs`

**Интерфейсы:**
- Использует: `InstallerApp::log: String`
- Использует: `fn can_copy_log(log: &str) -> bool`
- Использует: `fn copy_log_to_clipboard(ctx: &egui::Context, log: &str)`
- Создаёт: `fn show_log_field(ui: &mut egui::Ui, log: &str) -> LogFieldOutput`
- Создаёт: `fn show_details(ui: &mut egui::Ui, log: &str) -> DetailsUiOutput`

- [x] **Шаг 1: Написать падающие UI-тесты layout и доступности копирования**

Добавить в `crates/installer/tests/ui.rs` тест заполнения области и переноса
строк, а также тест недоступной кнопки при пустом журнале. Для layout-теста
использовать собственный `Context::run_ui` со стандартными шрифтами:
`egui::__run_test_ui` намеренно загружает пустой набор шрифтов, поэтому не
может измерить и перенести кириллицу.

```rust
#[test]
fn log_field_fills_available_space_and_wraps_long_lines() {
    let long_line = "очень длинная строка журнала ".repeat(80);
    let mut observed = None;

    run_sized_ui(egui::vec2(640.0, 480.0), |ui| {
        let output = show_log_field(ui, &long_line);
        observed = Some((
            output.response.rect.width(),
            output.response.rect.height(),
            output.galley.rows.len(),
        ));
    });

    let (width, height, rows) = observed.expect("test UI must render the log field");
    assert!(width >= 610.0, "log field width was {width}");
    assert!(height >= 450.0, "log field height was {height}");
    assert!(rows > 1, "long log line did not wrap");
}
```

- [x] **Шаг 2: Запустить UI-тесты и подтвердить состояние RED**

Выполнить:

```powershell
cargo test -p evohime-installer --test ui log_field_fills_available_space_and_wraps_long_lines
```

Ожидается: компиляция падает, потому что `show_log_field` и `show_details` ещё
не существуют.

- [x] **Шаг 3: Реализовать адаптивное поле журнала только для чтения**

Добавить публичные компоненты в `crates/installer/src/ui.rs`, затем
импортировать их в `crates/installer/src/main.rs`:

```rust
pub struct LogFieldOutput {
    pub response: egui::Response,
    pub text_response: egui::AtomLayoutResponse,
    pub galley: Arc<egui::Galley>,
}

pub struct DetailsUiOutput {
    pub copy_button: egui::Response,
    pub log_field: LogFieldOutput,
}
```

`show_log_field` заранее запоминает `ui.available_size()`, резервирует этот
размер для внешнего `Frame` и возвращает его `Response` вместе с galley
вложенного `TextEdit`. Это необходимо для egui 0.35: `TextEdit::min_size`
использует ширину, но не заполняет высоту в вертикальном `ScrollArea`.
`show_details` возвращает `DetailsUiOutput`, чтобы production-код проверял
`copy_button.clicked()`, а тест — `copy_button.enabled()`.

Неизменяемая реализация `TextBuffer` для `&str` оставляет поле интерактивным для выделения, но запрещает редактирование. Конечное значение `desired_width(ui.available_width())` сохраняет перенос многострочного текста; не заменять его на `f32::INFINITY`, потому что в egui 0.35 это отключает автоматический перенос.

- [x] **Шаг 4: Сделать окно и основной layout адаптивными**

Изменить `NativeOptions`:

```rust
viewport: egui::ViewportBuilder::default()
    .with_inner_size([900.0, 720.0])
    .with_min_inner_size([720.0, 720.0]),
```

В начале closure внешнего frame заставить содержимое заполнить клиентскую область:

```rust
.show(ui, |ui| {
    ui.set_min_size(ui.available_size());
    ui.add_space(8.0);
```

Сохранить существующие начальное, активное, ошибочное и завершённое состояния. Переместить сообщение о ярлыке при успехе выше подробностей, затем один раз вывести подробности после блока `if !self.started { ... } else { ... }`:

```rust
ui.add_space(12.0);
let details = show_details(ui, &self.log);
if details.copy_button.clicked() {
    copy_log_to_clipboard(ui.ctx(), &self.log);
}
```

Удалить старые `CollapsingHeader`, `ScrollArea` с `max_height(120.0)` и цикл по строкам журнала. Не изменять события прогресса, строки статуса, расчёт прогресса, установочные операции или сообщение о ярлыке.

- [x] **Шаг 5: Запустить layout-тест и полные тесты установщика**

Выполнить:

```powershell
cargo test -p evohime-installer --test ui log_field_fills_available_space_and_wraps_long_lines
cargo test -p evohime-installer --test ui
cargo test -p evohime-installer --test icacls_windows
cargo check -p evohime-installer
```

Ожидается: layout-тест наблюдает минимум `610×450` логических points и больше одной строки galley; все тесты установщика проходят.

- [x] **Шаг 6: Закоммитить адаптивный интерфейс**

```powershell
git add -- crates/installer/src/main.rs crates/installer/src/ui.rs crates/installer/tests/ui.rs docs/superpowers/plans/2026-07-28-installer-details-layout.md
git commit -m "fix(installer): expand and copy setup details"
```

---

### Задача 3: Полная проверка и визуальная приёмка

**Файлы:**
- Проверить: `crates/installer/src/main.rs`
- Проверить: `docs/superpowers/specs/2026-07-28-installer-details-layout-design.md`
- Проверить: `docs/superpowers/plans/2026-07-28-installer-details-layout.md`

**Интерфейсы:**
- Использует: готовый бинарник `target/release/evohime-setup.exe`
- Создаёт: подтверждённые форматирование, тесты, release-сборку, визуальную приёмку и чистый workspace без `target/`

- [ ] **Шаг 1: Запустить форматирование, тесты и компиляцию из актуального исходного кода**

Выполнить:

```powershell
cargo fmt --check
cargo test -p evohime-installer --test ui
cargo test -p evohime-installer --test icacls_windows
cargo check -p evohime-installer
cargo build --release -p evohime-installer --bin evohime-setup
```

Ожидается: каждая команда завершается с кодом `0`; тесты сообщают ноль падений; существует `target/release/evohime-setup.exe`.

- [ ] **Шаг 2: Проверить Windows GUI subsystem**

Выполнить:

```powershell
$exe = (Resolve-Path 'target\release\evohime-setup.exe').Path
$bytes = [System.IO.File]::ReadAllBytes($exe)
$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
$optionalHeader = $peOffset + 24
$subsystem = [BitConverter]::ToUInt16($bytes, $optionalHeader + 68)
if ($subsystem -ne 2) {
    throw "Expected Windows GUI subsystem 2, got $subsystem"
}
```

Ожидается: исключения нет; subsystem равен `2`.

- [ ] **Шаг 3: Запустить и визуально проверить начальное окно**

Запустить проверенный release-бинарник, не нажимая «Установить»:

```powershell
$setupProcess = Start-Process -FilePath (Resolve-Path 'target\release\evohime-setup.exe') -PassThru
```

Через управление приложениями Windows проверить:

- начальная клиентская область открывается с размером `900×720` логических points;
- клиентскую область нельзя уменьшить ниже `720×720`;
- «Подробности» видны до начала установки;
- пустое поле показывает «Журнал пока пуст.»;
- «Копировать всё» недоступна при пустом журнале;
- увеличение ширины и высоты расширяет поле по обеим осям;
- поле подробностей не перекрывает шапку, кнопку запуска, область статуса или индикатор прогресса;
- закрытие окна установщика завершает `$setupProcess`.

- [ ] **Шаг 4: Проверить заполненный журнал автоматизированным UI-покрытием**

Повторно запустить тесты канонического буфера, границы clipboard API, размеров и переноса:

```powershell
cargo test -p evohime-installer --test ui appends_progress_entries_without_changing_their_text
cargo test -p evohime-installer --test ui copy_action_emits_the_exact_canonical_log_text
cargo test -p evohime-installer --test ui log_field_fills_available_space_and_wraps_long_lines
```

Ожидается: все три теста проходят. Проверить код и убедиться, что постоянный вызов `show_details` остаётся после сообщения завершённого состояния, поэтому завершённый и ошибочный журналы видимы до закрытия окна.

- [ ] **Шаг 5: Проверить финальный diff и удалить артефакты сборки**

Выполнить:

```powershell
git diff --check
git status --short
$workspace = (Resolve-Path '.').Path
$targetPath = (Resolve-Path 'target').Path
if (-not $targetPath.StartsWith($workspace + [System.IO.Path]::DirectorySeparatorChar)) {
    throw "Refusing to remove target outside workspace: $targetPath"
}
Remove-Item -LiteralPath $targetPath -Recurse -Force
git status --short
```

Ожидается: `git diff --check` не находит ошибок; до коммитов присутствуют только файлы текущей задачи; `target/` удалён; cleanup не изменил отслеживаемые файлы репозитория.

- [ ] **Шаг 6: Зафиксировать результат проверки без пустого коммита**

Выполнить:

```powershell
git log -5 --oneline
git status --short --branch
```

Ожидается: два коммита реализации и коммиты планирования присутствуют в истории, текущая ветка — `main`, рабочее дерево чистое. Без прямого запроса пользователя ничего не пушить.
