use eframe::egui;
use std::sync::Arc;

pub struct LogFieldOutput {
    pub response: egui::Response,
    pub text_response: egui::AtomLayoutResponse,
    pub galley: Arc<egui::Galley>,
}

pub struct DetailsUiOutput {
    pub copy_button: egui::Response,
    pub log_field: LogFieldOutput,
}

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

pub fn show_log_field(ui: &mut egui::Ui, log: &str) -> LogFieldOutput {
    let available_size = ui.available_size();
    let content_size = egui::vec2(
        (available_size.x - 16.0).max(0.0),
        (available_size.y - 16.0).max(0.0),
    );

    let frame_output = egui::Frame::new()
        .fill(egui::Color32::from_rgb(18, 18, 23))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(58)))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_min_size(content_size);

            egui::ScrollArea::vertical()
                .id_salt("installer-details-log")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let mut read_only_log = log;

                    egui::TextEdit::multiline(&mut read_only_log)
                        .id_salt("installer-details-text")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(ui.available_width())
                        .frame(egui::Frame::NONE)
                        .hint_text("Журнал пока пуст.")
                        .show(ui)
                })
                .inner
        });

    LogFieldOutput {
        response: frame_output.response,
        text_response: frame_output.inner.response,
        galley: frame_output.inner.galley,
    }
}

pub fn show_details(ui: &mut egui::Ui, log: &str) -> DetailsUiOutput {
    let copy_button = ui
        .horizontal(|ui| {
            ui.label(egui::RichText::new("Подробности").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled(can_copy_log(log), egui::Button::new("Копировать всё"))
            })
            .inner
        })
        .inner;

    ui.add_space(8.0);
    let log_field = show_log_field(ui, log);

    DetailsUiOutput {
        copy_button,
        log_field,
    }
}
