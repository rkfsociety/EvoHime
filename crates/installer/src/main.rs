#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! EvoHime Installer (evohime-setup.exe) — Фаза 3 плана.
//!
//! Единственный экран прогресса на egui (единый GUI-стек с Launcher'ом,
//! раздел I плана). Вся установка выполняется на фоновом Tokio-рантайме;
//! прогресс приходит в UI-поток через mpsc-канал, чтобы не блокировать
//! отрисовку окна во время сетевых операций/распаковки.

use chrono::Local;
use eframe::egui;
use evohime_artifacts::{download_fresh_and_verify, extract_zip};
use evohime_installer::ui::{append_log_entry, copy_log_to_clipboard, show_details};
use evohime_installer::{
    clear_dirty_installation_safely_observed, create_shortcut_observed, format_elapsed,
    is_installation_dirty, mark_setup_complete, redact_command_event,
    restrict_to_current_user_observed, InstallationTiming,
};
use evohime_launcher::config::{self, DbConfig};
use evohime_launcher::observed_command::{CommandEvent, CommandStream};
use evohime_launcher::{
    apply_migrations, build_dsn, generate_password, patch_pg_hba_trust_local, postgres,
};
use evohime_win_support::{free_bytes_available, SingleInstanceLock};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

const GITHUB_REPO: &str = "rkfsociety/EvoHime";
const MIN_FREE_BYTES: u64 = 1_500_000_000; // ~1.5 ГБ, раздел VI плана
const DB_NAME: &str = "evohime";

/// Один шаг прогресса, отправляемый из фоновой установки в UI-поток.
#[derive(Debug, Clone)]
enum ProgressEvent {
    Stage(String),
    Operation(String),
    Command(CommandEvent),
    Error(String),
    Done,
}

// Приблизительное число шагов установки (см. run_installation_fallible) —
// используется только для длины прогресс-бара, не для точного подсчёта:
// часть шагов условны (напр. очистка "грязной" установки), так что бар
// иногда не дойдёт до 100% ровно на предпоследнем шаге — это нормально,
// на Done прогресс принудительно выставляется в 1.0.
const APPROX_TOTAL_STEPS: usize = 20;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(122, 162, 255);
const ACCENT_DARK: egui::Color32 = egui::Color32::from_rgb(44, 60, 104);
const ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 120, 120);
const SUCCESS_COLOR: egui::Color32 = egui::Color32::from_rgb(120, 220, 150);
const DIM_TEXT: egui::Color32 = egui::Color32::from_gray(150);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(28, 29, 36);
const SURFACE_ALT: egui::Color32 = egui::Color32::from_rgb(35, 36, 45);

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 720.0])
            .with_min_inner_size([720.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "EvoHime Setup",
        options,
        Box::new(|cc| {
            apply_style(&cc.egui_ctx);
            Ok(Box::new(InstallerApp::new()))
        }),
    )
}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
    visuals.selection.bg_fill = ACCENT;
    visuals.hyperlink_color = ACCENT;
    visuals.panel_fill = SURFACE;
    visuals.window_fill = SURFACE;
    visuals.faint_bg_color = SURFACE_ALT;
    visuals.extreme_bg_color = egui::Color32::from_rgb(18, 18, 23);
    visuals.widgets.noninteractive.bg_fill = SURFACE_ALT;
    visuals.widgets.inactive.bg_fill = SURFACE_ALT;
    visuals.widgets.hovered.bg_fill = ACCENT_DARK;
    visuals.widgets.active.bg_fill = ACCENT;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 12.0);
    style.spacing.button_padding = egui::vec2(18.0, 10.0);
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(232, 233, 240));
    ctx.set_style_of(egui::Theme::Dark, style);
}

struct InstallerApp {
    rx: Option<mpsc::Receiver<ProgressEvent>>,
    current_stage: String,
    log: String,
    steps_done: usize,
    finished: bool,
    failed: bool,
    started: bool,
    timing: Option<InstallationTiming>,
}

impl InstallerApp {
    fn new() -> Self {
        Self {
            rx: None,
            current_stage: "Готов к установке.".to_string(),
            log: String::new(),
            steps_done: 0,
            finished: false,
            failed: false,
            started: false,
            timing: None,
        }
    }

    fn start_installation(&mut self) {
        self.started = true;
        self.timing = Some(InstallationTiming::started(Instant::now()));
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to start Tokio runtime");
            runtime.block_on(run_installation(tx));
        });
    }

    fn progress_fraction(&self) -> f32 {
        if self.finished {
            1.0
        } else {
            (self.steps_done as f32 / APPROX_TOTAL_STEPS as f32).clamp(0.0, 0.97)
        }
    }
}

impl eframe::App for InstallerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    ProgressEvent::Stage(msg) => {
                        self.steps_done += 1;
                        self.current_stage = msg.clone();
                        if let Some(timing) = &mut self.timing {
                            timing.begin_stage(Instant::now());
                        }
                        append_log_entry(&mut self.log, &timestamped(&msg));
                    }
                    ProgressEvent::Operation(msg) => {
                        append_log_entry(&mut self.log, &timestamped(&msg));
                    }
                    ProgressEvent::Command(event) => {
                        append_log_entry(&mut self.log, &format_command_event(event));
                    }
                    ProgressEvent::Error(msg) => {
                        self.current_stage = "Установка прервана из-за ошибки.".to_string();
                        append_log_entry(&mut self.log, &timestamped(&format!("Ошибка: {msg}")));
                        if let Some(timing) = &mut self.timing {
                            timing.finish(Instant::now());
                        }
                        self.failed = true;
                    }
                    ProgressEvent::Done => {
                        self.current_stage = "Готово!".to_string();
                        if let Some(timing) = &mut self.timing {
                            timing.finish(Instant::now());
                        }
                        self.finished = true;
                    }
                }
            }
        }

        egui::Frame::new()
            .fill(egui::Color32::from_rgb(22, 23, 29))
            .corner_radius(egui::CornerRadius::same(14))
            .inner_margin(egui::Margin::symmetric(22, 20))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.heading(egui::RichText::new("EvoHime").size(30.0).color(ACCENT));
                    ui.label(
                        egui::RichText::new("Автоматическая установка компонентов")
                            .size(11.0)
                            .color(DIM_TEXT),
                    );
                    ui.label(egui::RichText::new("Установка").color(DIM_TEXT));
                });
                ui.add_space(16.0);

                if !self.started {
                    ui.vertical_centered(|ui| {
                        ui.label("Нажмите «Установить», чтобы начать.");
                        ui.add_space(12.0);
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("Установить").size(16.0),
                            ))
                            .clicked()
                        {
                            self.start_installation();
                        }
                    });
                } else {
                    let (bar_color, status_text, status_color) = if self.failed {
                        (ERROR_COLOR, "Установка не завершена", ERROR_COLOR)
                    } else if self.finished {
                        (SUCCESS_COLOR, "Установка завершена", SUCCESS_COLOR)
                    } else {
                        (ACCENT, "Устанавливаю...", DIM_TEXT)
                    };

                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(status_text)
                                .size(15.0)
                                .color(status_color),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(&self.current_stage)
                                .color(DIM_TEXT)
                                .size(13.0),
                        );
                        if let Some(timing) = &self.timing {
                            let elapsed = timing.elapsed(Instant::now());
                            ui.label(
                                egui::RichText::new(format!(
                                    "Всего {} · этап {}",
                                    format_elapsed(elapsed.total),
                                    format_elapsed(elapsed.stage)
                                ))
                                .color(DIM_TEXT)
                                .size(12.0),
                            );
                        }
                    });

                    ui.add_space(10.0);
                    ui.add(
                        egui::ProgressBar::new(self.progress_fraction())
                            .desired_height(10.0)
                            .fill(bar_color)
                            .corner_radius(5.0),
                    );
                    ui.add_space(12.0);

                    if self.finished {
                        ui.add_space(6.0);
                        ui.vertical_centered(|ui| {
                            ui.label("Запустите ярлык «EvoHime Launcher» на рабочем столе.");
                        });
                    }
                }

                ui.add_space(12.0);
                let details = show_details(ui, &self.log);
                if details.copy_button.clicked() {
                    copy_log_to_clipboard(ui.ctx(), &self.log);
                }
            });

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(150));
    }
}

fn timestamped(message: &str) -> String {
    format!("[{}] {message}", Local::now().format("%H:%M:%S"))
}

fn format_command_event(event: CommandEvent) -> String {
    match event {
        CommandEvent::Started { display } => timestamped(&format!("> {display}")),
        CommandEvent::Output { stream, line } => {
            let stream = match stream {
                CommandStream::Stdout => "stdout",
                CommandStream::Stderr => "stderr",
            };
            format!("[{stream}] {line}")
        }
        CommandEvent::Finished {
            success,
            exit_code,
            elapsed,
        } => {
            let result = if success {
                "Команда завершена успешно".to_string()
            } else {
                match exit_code {
                    Some(code) => format!("Команда завершена с кодом {code}"),
                    None => "Не удалось выполнить команду".to_string(),
                }
            };
            timestamped(&format!("{result} ({:.1} с)", elapsed.as_secs_f64()))
        }
    }
}

fn send_command_event(tx: &mpsc::Sender<ProgressEvent>, event: CommandEvent, secrets: &[&str]) {
    let event = redact_command_event(event, secrets);
    let _ = tx.send(ProgressEvent::Command(event));
}

fn install_dir() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data).join("EvoHime")
}

async fn run_installation(tx: mpsc::Sender<ProgressEvent>) {
    if let Err(err) = run_installation_fallible(&tx).await {
        let _ = tx.send(ProgressEvent::Error(err.to_string()));
    }
}

async fn run_installation_fallible(tx: &mpsc::Sender<ProgressEvent>) -> anyhow::Result<()> {
    let stage = |msg: &str| {
        let _ = tx.send(ProgressEvent::Stage(msg.to_string()));
    };
    let operation = |msg: &str| {
        let _ = tx.send(ProgressEvent::Operation(msg.to_string()));
    };

    // 1. Предотвратить параллельный запуск двух Installer'ов (раздел XII
    //    плана, ответ на вопрос "что если запустят два инсталлятора сразу").
    stage("Проверка, не запущена ли уже установка...");
    let _install_lock = SingleInstanceLock::acquire("EvoHimeInstallerMutex")
        .map_err(|err| anyhow::anyhow!("установка уже выполняется: {err}"))?;

    let install_dir = install_dir();

    // 2. Защита от прерванной установки (раздел VI плана): "грязная" папка
    //    от прошлой неудачной попытки удаляется полностью.
    if is_installation_dirty(&install_dir) {
        stage("Обнаружена незавершённая установка...");
        clear_dirty_installation_safely_observed(
            &install_dir,
            |message| stage(message),
            |event| send_command_event(tx, event, &[]),
        )
        .await
        .map_err(|err| {
            anyhow::anyhow!(
                "не удалось очистить незавершённую установку {}: {err}",
                install_dir.display()
            )
        })?;
        operation("Незавершённая установка полностью очищена.");
    }
    tokio::fs::create_dir_all(&install_dir).await?;

    // 3. Проверка свободного места (раздел VI/XIV плана).
    stage("Проверка свободного места на диске...");
    let free = free_bytes_available(&install_dir)
        .map_err(|err| anyhow::anyhow!("не удалось проверить диск: {err}"))?;
    if free < MIN_FREE_BYTES {
        anyhow::bail!(
            "недостаточно места на диске: доступно {} МБ, требуется минимум {} МБ",
            free / 1_000_000,
            MIN_FREE_BYTES / 1_000_000
        );
    }

    let client = reqwest::Client::new();

    // 4. Скачивание релиза (server.zip, launcher.zip, dist.zip,
    //    migrations.zip, worker.zip) + проверка SHA256 (раздел IV/VI плана).
    //    Единственный exe-файл, который пользователь скачивает и запускает
    //    сам, — это evohime-setup.exe; все остальные компоненты — архивы,
    //    которые Installer распаковывает сам.
    stage("Загрузка последнего релиза...");
    let versions_dir = install_dir.join("versions").join("current");
    tokio::fs::create_dir_all(&versions_dir).await?;

    for asset_name in [
        "server.zip",
        "launcher.zip",
        "dist.zip",
        "migrations.zip",
        "worker.zip",
    ] {
        let url = format!("https://github.com/{GITHUB_REPO}/releases/latest/download/{asset_name}");
        let sha_url = format!("{url}.sha256");
        let dest = versions_dir.join(asset_name);

        stage(&format!("Скачивание {asset_name}..."));
        let ok = download_fresh_and_verify(&client, &url, &sha_url, &dest).await?;
        if !ok {
            anyhow::bail!("SHA256 не совпадает для {asset_name} — прерываю установку");
        }
        operation(&format!(
            "{asset_name} скачан заново, SHA256 подтверждён: {}",
            dest.display()
        ));
    }

    // 4b. PostgreSQL не версионируется вместе с остальным релизом (как и
    //     launcher.zip) — скачивается один раз при установке, а не при
    //     каждом автообновлении.
    let pg_dir = install_dir.join("pg16");
    tokio::fs::create_dir_all(&pg_dir).await?;
    let pg_zip_path = pg_dir.join("postgres.zip");
    {
        let url = format!("https://github.com/{GITHUB_REPO}/releases/latest/download/postgres.zip");
        let sha_url = format!("{url}.sha256");

        stage("Скачивание PostgreSQL...");
        let ok = download_fresh_and_verify(&client, &url, &sha_url, &pg_zip_path).await?;
        if !ok {
            anyhow::bail!("SHA256 не совпадает для postgres.zip — прерываю установку");
        }
        operation(&format!(
            "postgres.zip скачан заново, SHA256 подтверждён: {}",
            pg_zip_path.display()
        ));
    }

    // 5. Распаковка компонентов: server.zip — прямо в корень версии (даёт
    //    <version>/server.exe), launcher.zip — вне versions_dir, в
    //    install_dir/launcher (общий для всех версий), postgres.zip — в
    //    install_dir/pg16 (даёт pg16/bin, pg16/lib, pg16/share), остальные —
    //    в свои подпапки внутри версии.
    stage("Распаковка компонентов...");
    for (zip_name, dest) in [
        ("server.zip", versions_dir.clone()),
        ("dist.zip", versions_dir.join("dist")),
        ("migrations.zip", versions_dir.join("migrations")),
        ("worker.zip", versions_dir.join("worker")),
        ("launcher.zip", install_dir.join("launcher")),
    ] {
        let zip_path = versions_dir.join(zip_name);
        let destination = dest.display().to_string();
        tokio::task::spawn_blocking(move || extract_zip(&zip_path, &dest)).await??;
        operation(&format!("{zip_name} распакован: {destination}"));
    }
    {
        let dest = pg_dir.clone();
        let destination = dest.display().to_string();
        tokio::task::spawn_blocking(move || extract_zip(&pg_zip_path, &dest)).await??;
        operation(&format!("postgres.zip распакован: {destination}"));
    }

    // 6. Инициализация PostgreSQL: фикс прав через icacls (раздел III/VI
    //    плана) на пустой data-каталог (initdb требует отсутствия
    //    наследуемых прав), initdb под текущим Windows-пользователем,
    //    trust-аутентификация для локальных подключений (нет
    //    многопользовательской экспозиции, которую нужно защищать —
    //    см. pg_auth.rs), запуск кластера и создание базы.
    let pg_bin_dir = pg_dir.join("bin");
    let pg_data_dir = pg_dir.join("data");
    tokio::fs::create_dir_all(&pg_data_dir).await?;
    let mut command_observer = |event| send_command_event(tx, event, &[]);
    restrict_to_current_user_observed(&pg_data_dir, &mut command_observer).await?;

    stage("Генерация пароля базы данных...");
    let db_password = generate_password(24);
    let db_user = std::env::var("USERNAME").unwrap_or_else(|_| "evohime".to_string());

    stage("Инициализация базы данных (initdb)...");
    postgres::initdb_observed(&pg_bin_dir, &pg_data_dir, &db_user, &db_password, |event| {
        send_command_event(tx, event, &[db_password.as_str()])
    })
    .await?;

    let pg_hba_path = pg_data_dir.join("pg_hba.conf");
    stage("Настройка аутентификации PostgreSQL...");
    patch_pg_hba_trust_local(&pg_hba_path).await?;
    operation(&format!(
        "Локальная аутентификация PostgreSQL настроена: {}",
        pg_hba_path.display()
    ));

    stage("Запуск PostgreSQL...");
    postgres::start_observed(&pg_bin_dir, &pg_data_dir, postgres::PG_PORT, |event| {
        send_command_event(tx, event, &[db_password.as_str()])
    })
    .await?;

    stage("Создание базы данных...");
    postgres::create_database_if_missing(&db_user, &db_password, postgres::PG_PORT, DB_NAME)
        .await?;
    operation(&format!("База данных {DB_NAME} готова."));

    config::save(
        &install_dir,
        &DbConfig {
            user: db_user.clone(),
            password: db_password.clone(),
            port: postgres::PG_PORT,
            db_name: DB_NAME.to_string(),
        },
    )
    .await?;
    operation(&format!(
        "Конфигурация базы данных сохранена: {}",
        install_dir
            .join("launcher-data")
            .join("config.json")
            .display()
    ));

    // 7. Применение миграций программно (раздел III плана, без sqlx-cli).
    stage("Применение миграций базы данных...");
    let dsn = build_dsn(
        &db_user,
        &db_password,
        "127.0.0.1",
        postgres::PG_PORT,
        DB_NAME,
    );
    let migrations_dir = versions_dir.join("migrations");
    if migrations_dir.exists() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&dsn)
            .await?;
        apply_migrations(&pool, &migrations_dir).await?;
        operation(&format!(
            "Миграции базы данных применены: {}",
            migrations_dir.display()
        ));
    } else {
        operation("Каталог миграций отсутствует, применение пропущено.");
    }

    // 8. Ярлык на рабочем столе (раздел VI плана).
    stage("Создание ярлыка...");
    let desktop_dir = dirs_desktop();
    let shortcut_path = desktop_dir.join("EvoHime Launcher.lnk");
    let launcher_exe = install_dir.join("launcher").join("evohime-launcher.exe");
    if let Some(parent) = shortcut_path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    match create_shortcut_observed(&shortcut_path, &launcher_exe, |event| {
        send_command_event(tx, event, &[])
    })
    .await
    {
        Ok(()) => operation(&format!("Ярлык создан: {}", shortcut_path.display())),
        Err(error) => operation(&format!("Не удалось создать ярлык: {error}")),
    }

    // 9. Финальный маркер — строго последним шагом (раздел VI плана).
    stage("Финализация установки...");
    mark_setup_complete(&install_dir).await?;
    operation("Маркер завершённой установки записан.");

    let _ = tx.send(ProgressEvent::Done);
    Ok(())
}

fn dirs_desktop() -> PathBuf {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(user_profile).join("Desktop")
}
