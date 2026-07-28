use eframe::egui;

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
