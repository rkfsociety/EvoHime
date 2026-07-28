//! Фаза 2.5 плана: spike, проверяющий связку egui (через eframe/winit) +
//! системный трей (tray-icon) на Windows *до* того, как весь Launcher
//! проектируется вокруг этой пары.
//!
//! Риск, который этот прототип проверяет: оба компонента управляют
//! Win32-окнами (winit — видимым окном, tray-icon — скрытым окном для
//! обработки сообщений трея) и могут конфликтовать через один и тот же
//! message loop потока. Известный рабочий паттерн — создать `TrayIcon` на
//! том же потоке, что будет запускать `eframe::run_native` (единый message
//! loop обслуживает оба окна), и вычитывать события трея внутри
//! `eframe::App::ui`, которая вызывается каждый кадр благодаря
//! `request_repaint_after`.
//!
//! Запуск: `cargo run -p evohime-launcher --bin spike_tray`
//! Успех спайка = окно открывается, трей-иконка появляется, клик по
//! пункту меню трея отражается в тексте окна — без зависаний/паники.

use eframe::egui;
use std::sync::mpsc;

fn main() -> eframe::Result<()> {
    // Трей-иконка создаётся на текущем (главном) потоке — до входа в
    // eframe::run_native, который на Windows забирает этот поток под свой
    // message loop. Если бы TrayIcon создавался на отдельном потоке, его
    // скрытое окно получило бы message loop другого потока — тоже рабочий
    // вариант, но именно "тот же поток, что и eframe" — сценарий по
    // умолчанию для Launcher'а (раздел VII плана), поэтому спайк проверяет
    // именно его.
    let icon = solid_color_icon();

    let menu = tray_icon::menu::Menu::new();
    let ping_item =
        tray_icon::menu::MenuItem::new("Ping (proves menu clicks reach us)", true, None);
    menu.append(&ping_item)
        .expect("failed to append menu item to tray menu");

    let tray_icon = tray_icon::TrayIconBuilder::new()
        .with_tooltip("EvoHime Launcher — spike")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()
        .expect("failed to build tray icon — this itself is spike signal #1");

    let (status_tx, status_rx) = mpsc::channel::<String>();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([360.0, 160.0]),
        ..Default::default()
    };

    eframe::run_native(
        "EvoHime Launcher — egui+tray spike",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(SpikeApp {
                _tray_icon: tray_icon,
                ping_item_id: ping_item.id().clone(),
                status_tx,
                status_rx,
                last_status: "waiting for tray events...".to_string(),
            }))
        }),
    )
}

struct SpikeApp {
    // Держим TrayIcon живым на протяжении всего приложения — если его
    // уронить (Drop), иконка исчезнет из трея.
    _tray_icon: tray_icon::TrayIcon,
    ping_item_id: tray_icon::menu::MenuId,
    status_tx: mpsc::Sender<String>,
    status_rx: mpsc::Receiver<String>,
    last_status: String,
}

impl eframe::App for SpikeApp {
    // eframe 0.35 передаёт готовый `&mut Ui` (CentralPanel уже развёрнут),
    // а не `&egui::Context` — метод трейта называется `ui`, не `update`.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Спайк-сигнал #2: вычитываем события трея (клик по иконке) и меню
        // (клик по пункту) на каждом кадре — если бы message loop винита и
        // трея конфликтовали, эти каналы либо не наполнялись бы вовсе,
        // либо приложение бы зависло здесь.
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            let _ = self.status_tx.send(format!("tray icon event: {event:?}"));
        }
        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == self.ping_item_id {
                let _ = self.status_tx.send("menu: Ping clicked".to_string());
            } else {
                let _ = self.status_tx.send(format!("menu event: {event:?}"));
            }
        }
        while let Ok(status) = self.status_rx.try_recv() {
            self.last_status = status;
        }

        ui.heading("egui + tray-icon spike");
        ui.label("Right-click the tray icon and click \"Ping\".");
        ui.separator();
        ui.label(format!("Last event: {}", self.last_status));

        // Держим кадры тикающими, чтобы события трея вычитывались с низкой
        // задержкой даже когда окно не в фокусе/без ввода.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(150));
    }
}

/// Генерирует простую 16x16 RGBA-иконку в памяти — для спайка не нужен
/// файл ассета.
fn solid_color_icon() -> tray_icon::Icon {
    let size = 16u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for _ in 0..(size * size) {
        rgba.extend_from_slice(&[0, 200, 0, 255]);
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("failed to build in-memory tray icon")
}
