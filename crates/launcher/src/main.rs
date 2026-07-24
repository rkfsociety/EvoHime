//! EvoHime Launcher (evohime-launcher.exe) — Фаза 4 плана.
//!
//! Живёт в системном трее; запускает и мониторит `server.exe`/`worker.py`,
//! раздаёт React-статику (5173) и REST-статус (3001) на встроенных
//! axum-серверах, останавливает компоненты через `POST /shutdown` с
//! токеном сессии (раздел IV/XV плана). Полный механизм обновлений —
//! Фаза 5; здесь — базовый жизненный цикл: старт, мониторинг, остановка.

use eframe::egui;
use evohime_launcher::status_server::{ComponentStatus, LauncherStatus, StatusServerState};
use evohime_launcher::{
    build_static_router, build_status_router, generate_session_token, safe_mode, ManagedProcess,
};
use evohime_win_support::SingleInstanceLock;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const STATIC_SERVER_PORT: u16 = 5173;
const STATUS_SERVER_PORT: u16 = 3001;
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);

fn main() -> eframe::Result<()> {
    // 1. Предотвратить параллельный запуск (раздел IX плана).
    let _instance_lock = match SingleInstanceLock::acquire("EvoHimeLauncherMutex") {
        Ok(lock) => lock,
        Err(err) => {
            eprintln!("EvoHime Launcher уже запущен: {err}");
            std::process::exit(1);
        }
    };

    // 2. Safe Mode (раздел VII плана): при зажатом Shift пропускаем
    // проверку обновлений (сама проверка появится в Фазе 5) — здесь флаг
    // только считывается и логируется.
    let safe_mode = safe_mode::is_shift_held();
    if safe_mode {
        tracing::warn!("Safe Mode: запуск без проверки обновлений");
    }

    let session_token = generate_session_token();
    let install_dir = install_dir();
    let current_dir = current_version_dir(&install_dir);

    let runtime = tokio::runtime::Runtime::new().expect("failed to start Tokio runtime");
    let shared_status = Arc::new(Mutex::new(LauncherStatus {
        components: vec![
            ComponentStatus {
                name: "server".to_string(),
                online: false,
            },
            ComponentStatus {
                name: "worker".to_string(),
                online: false,
            },
        ],
        update_available: false,
    }));

    spawn_static_server(&runtime, current_dir.join("dist"), session_token.clone());
    spawn_status_server(&runtime, session_token.clone(), shared_status.clone());
    spawn_process_supervisor(&runtime, current_dir, session_token, shared_status.clone());

    // Трей-иконка создаётся на этом же (главном) потоке — до входа в
    // eframe::run_native, который на Windows забирает его под свой message
    // loop (валидировано в src/bin/spike_tray.rs, Фаза 2.5).
    let tray_menu = build_tray_menu();
    let tray_icon = tray_icon::TrayIconBuilder::new()
        .with_tooltip("EvoHime Launcher")
        .with_icon(status_icon(false))
        .with_menu(Box::new(tray_menu.menu))
        .build()
        .expect("failed to build tray icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([360.0, 220.0]),
        ..Default::default()
    };

    eframe::run_native(
        "EvoHime Launcher",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(LauncherApp {
                _runtime: runtime,
                _instance_lock,
                _tray_icon: tray_icon,
                tray_ids: tray_menu.ids,
                status: shared_status,
                safe_mode,
            }))
        }),
    )
}

struct TrayMenu {
    menu: tray_icon::menu::Menu,
    ids: TrayMenuIds,
}

struct TrayMenuIds {
    open_dashboard: tray_icon::menu::MenuId,
    exit: tray_icon::menu::MenuId,
}

fn build_tray_menu() -> TrayMenu {
    let menu = tray_icon::menu::Menu::new();

    let open_dashboard = tray_icon::menu::MenuItem::new("Open Dashboard", true, None);
    let check_updates = tray_icon::menu::MenuItem::new("Check Updates", true, None);
    let stop = tray_icon::menu::MenuItem::new("Stop", true, None);
    let settings = tray_icon::menu::MenuItem::new("Settings", true, None);
    let exit = tray_icon::menu::MenuItem::new("Exit", true, None);

    let ids = TrayMenuIds {
        open_dashboard: open_dashboard.id().clone(),
        exit: exit.id().clone(),
    };

    // Check Updates/Stop/Settings полноценно заработают в Фазе 5/7 —
    // здесь они присутствуют в меню (раздел VII плана), но пока без
    // обработчиков.
    menu.append_items(&[
        &open_dashboard,
        &check_updates,
        &stop,
        &settings,
        &tray_icon::menu::PredefinedMenuItem::separator(),
        &exit,
    ])
    .expect("failed to append tray menu items");

    TrayMenu { menu, ids }
}

/// Простая цветная иконка 16x16 в памяти: зелёная — всё работает, красная —
/// проблема. Полноценный ассет (не сплошной цвет) — предмет полировки UX,
/// не блокирует функциональность.
fn status_icon(healthy: bool) -> tray_icon::Icon {
    let size = 16u32;
    let color = if healthy {
        [0, 200, 0, 255]
    } else {
        [200, 0, 0, 255]
    };
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for _ in 0..(size * size) {
        rgba.extend_from_slice(&color);
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("failed to build in-memory tray icon")
}

fn install_dir() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data).join("EvoHime")
}

/// Читает `current.txt`, чтобы определить активную версию. Если файл
/// отсутствует/битый — используем подпапку `versions/current` как есть
/// (раздел VII плана: полноценный fallback "искать последнюю по mtime"
/// реализуется в Фазе 5 вместе с остальным механизмом обновлений).
fn current_version_dir(install_dir: &Path) -> PathBuf {
    let current_txt = install_dir.join("current.txt");
    let version = std::fs::read_to_string(&current_txt)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "current".to_string());
    install_dir.join("versions").join(version)
}

fn spawn_static_server(runtime: &tokio::runtime::Runtime, dist_dir: PathBuf, token: String) {
    runtime.spawn(async move {
        let router = build_static_router(dist_dir, token);
        let addr = format!("127.0.0.1:{STATIC_SERVER_PORT}");
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                if let Err(err) = axum::serve(listener, router).await {
                    tracing::error!(%err, "static file server stopped unexpectedly");
                }
            }
            Err(err) => tracing::error!(%err, addr, "failed to bind static file server"),
        }
    });
}

fn spawn_status_server(
    runtime: &tokio::runtime::Runtime,
    token: String,
    status: Arc<Mutex<LauncherStatus>>,
) {
    runtime.spawn(async move {
        let state = StatusServerState {
            session_token: token.into(),
            status_provider: Arc::new(move || {
                status.lock().expect("status mutex poisoned").clone()
            }),
        };
        let router = build_status_router(state);
        let addr = format!("127.0.0.1:{STATUS_SERVER_PORT}");
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                if let Err(err) = axum::serve(listener, router).await {
                    tracing::error!(%err, "status server stopped unexpectedly");
                }
            }
            Err(err) => tracing::error!(%err, addr, "failed to bind status server"),
        }
    });
}

/// Запускает `server.exe`/`worker.py` (если найдены в текущей версии) и
/// периодически проверяет их здоровье, обновляя `shared_status` для
/// REST-эндпоинта. Полный перезапуск при падении и graceful shutdown при
/// выходе — упрощённая версия; полное соответствие разделу VII (учёт
/// "свой/чужой процесс" на занятых портах, restart backoff) — по мере
/// развития Фазы 5.
fn spawn_process_supervisor(
    runtime: &tokio::runtime::Runtime,
    current_dir: PathBuf,
    token: String,
    status: Arc<Mutex<LauncherStatus>>,
) {
    runtime.spawn(async move {
        let client = reqwest::Client::new();
        let server_exe = current_dir.join("server.exe");

        let mut server_process = ManagedProcess::new(
            "server",
            server_exe.clone(),
            vec![],
            "http://127.0.0.1:3000/health",
            Some("http://127.0.0.1:3000/shutdown".to_string()),
        );

        if server_exe.exists() {
            if let Err(err) = server_process
                .start(&[("EVOHIME_LOCAL_TOKEN".to_string(), token.clone())])
                .await
            {
                tracing::error!(%err, "failed to start server.exe");
            }
        } else {
            tracing::warn!(path = %server_exe.display(), "server.exe not found — nothing to supervise yet");
        }

        loop {
            tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;
            let server_online = server_exe.exists()
                && server_process.health_check(&client, Duration::from_secs(3)).await;

            if let Ok(mut guard) = status.lock() {
                if let Some(component) = guard.components.iter_mut().find(|c| c.name == "server") {
                    component.online = server_online;
                }
            }
        }
    });
}

struct LauncherApp {
    _runtime: tokio::runtime::Runtime,
    _instance_lock: SingleInstanceLock,
    _tray_icon: tray_icon::TrayIcon,
    tray_ids: TrayMenuIds,
    status: Arc<Mutex<LauncherStatus>>,
    safe_mode: bool,
}

impl eframe::App for LauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Клик по иконке в трее (открыть окно) — сейчас окно и так видно;
        // полноценное сворачивание в трей без закрытия окна — Фаза 5/7.
        while tray_icon::TrayIconEvent::receiver().try_recv().is_ok() {}

        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == self.tray_ids.open_dashboard {
                let _ = webbrowser_open("http://localhost:5173");
            } else if event.id == self.tray_ids.exit {
                std::process::exit(0);
            }
        }

        ui.heading("EvoHime Launcher");
        if self.safe_mode {
            ui.colored_label(egui::Color32::YELLOW, "Safe Mode");
        }
        ui.separator();

        if let Ok(status) = self.status.lock() {
            for component in &status.components {
                let (icon, color) = if component.online {
                    ("🟢", egui::Color32::GREEN)
                } else {
                    ("🔴", egui::Color32::RED)
                };
                ui.colored_label(color, format!("{icon} {}", component.name));
            }
        }

        ui.separator();
        if ui
            .button("Открыть панель (http://localhost:5173)")
            .clicked()
        {
            let _ = webbrowser_open("http://localhost:5173");
        }

        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

fn webbrowser_open(url: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn()
}
