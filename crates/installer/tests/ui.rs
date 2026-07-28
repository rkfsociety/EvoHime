use evohime_installer::ui::{
    append_log_entry, can_copy_log, copy_log_to_clipboard, show_details, show_log_field,
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

fn run_sized_ui(size: eframe::egui::Vec2, add_contents: impl FnMut(&mut eframe::egui::Ui)) {
    let ctx = eframe::egui::Context::default();
    let input = eframe::egui::RawInput {
        screen_rect: Some(eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            size,
        )),
        ..Default::default()
    };

    let _ = ctx.run_ui(input, add_contents);
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

#[test]
fn log_field_fills_available_space_and_wraps_long_lines() {
    let long_line = "очень длинная строка журнала ".repeat(80);
    let mut observed = None;

    run_sized_ui(eframe::egui::vec2(640.0, 480.0), |ui| {
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

#[test]
fn details_disable_copy_until_the_log_has_content() {
    let mut empty_enabled = None;
    run_sized_ui(eframe::egui::vec2(640.0, 480.0), |ui| {
        empty_enabled = Some(show_details(ui, "").copy_button.enabled());
    });

    let mut populated_enabled = None;
    run_sized_ui(eframe::egui::vec2(640.0, 480.0), |ui| {
        populated_enabled = Some(show_details(ui, "Запись журнала").copy_button.enabled());
    });

    assert_eq!(empty_enabled, Some(false));
    assert_eq!(populated_enabled, Some(true));
}
