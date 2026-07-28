use evohime_installer::ui::{append_log_entry, can_copy_log, copy_log_to_clipboard};

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
