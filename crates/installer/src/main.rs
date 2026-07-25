//! EvoHime Installer (evohime-setup.exe) — Фаза 3 плана.
//!
//! Единственный экран прогресса на egui (единый GUI-стек с Launcher'ом,
//! раздел I плана). Вся установка выполняется на фоновом Tokio-рантайме;
//! прогресс приходит в UI-поток через mpsc-канал, чтобы не блокировать
//! отрисовку окна во время сетевых операций/распаковки.
//!
//! ВАЖНО: реальная загрузка/инициализация portable PostgreSQL и Python
//! (initdb/pg_ctl/createdb) здесь ещё не реализована — точка расширения
//! зафиксирована явно в шаге 6 ниже (раздел X плана: "Фаза 8 — тестирование
//! на чистых VM" — там же проверяется весь цикл живьём, включая настоящие
//! сетевые загрузки многосотмегабайтных архивов, что невозможно достоверно
//! прогнать в рамках автоматической сборки этой сессии).

use eframe::egui;
use evohime_artifacts::{download_with_resume, extract_zip, verify_sha256};
use evohime_installer::{
    create_shortcut, is_installation_dirty, mark_setup_complete, restrict_to_current_user,
};
use evohime_launcher::{apply_migrations, build_dsn, generate_password, patch_pg_hba_trust_local};
use evohime_win_support::{free_bytes_available, SingleInstanceLock};
use std::path::PathBuf;
use std::sync::mpsc;

const GITHUB_REPO: &str = "rkfsociety/EvoHime";
const MIN_FREE_BYTES: u64 = 1_500_000_000; // ~1.5 ГБ, раздел VI плана

/// Один шаг прогресса, отправляемый из фоновой установки в UI-поток.
#[derive(Debug, Clone)]
enum ProgressEvent {
    Stage(String),
    Error(String),
    Done,
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([420.0, 260.0]),
        ..Default::default()
    };

    eframe::run_native(
        "EvoHime Setup",
        options,
        Box::new(|_cc| Ok(Box::new(InstallerApp::new()))),
    )
}

struct InstallerApp {
    rx: Option<mpsc::Receiver<ProgressEvent>>,
    log: Vec<String>,
    finished: bool,
    failed: bool,
    started: bool,
}

impl InstallerApp {
    fn new() -> Self {
        Self {
            rx: None,
            log: vec!["Нажмите «Установить», чтобы начать.".to_string()],
            finished: false,
            failed: false,
            started: false,
        }
    }

    fn start_installation(&mut self) {
        self.started = true;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to start Tokio runtime");
            runtime.block_on(run_installation(tx));
        });
    }
}

impl eframe::App for InstallerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    ProgressEvent::Stage(msg) => self.log.push(msg),
                    ProgressEvent::Error(msg) => {
                        self.log.push(format!("Ошибка: {msg}"));
                        self.failed = true;
                    }
                    ProgressEvent::Done => {
                        self.log
                            .push("Готово! Запустите ярлык «EvoHime Launcher».".to_string());
                        self.finished = true;
                    }
                }
            }
        }

        ui.heading("EvoHime — установка");
        ui.separator();

        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for line in &self.log {
                    ui.label(line);
                }
            });

        ui.separator();

        if !self.started {
            if ui.button("Установить").clicked() {
                self.start_installation();
            }
        } else if self.finished {
            ui.label("Установка завершена.");
        } else if self.failed {
            ui.label("Установка прервана из-за ошибки выше.");
        } else {
            ui.spinner();
        }

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(150));
    }
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

    // 1. Предотвратить параллельный запуск двух Installer'ов (раздел XII
    //    плана, ответ на вопрос "что если запустят два инсталлятора сразу").
    stage("Проверка, не запущена ли уже установка...");
    let _install_lock = SingleInstanceLock::acquire("EvoHimeInstallerMutex")
        .map_err(|err| anyhow::anyhow!("установка уже выполняется: {err}"))?;

    let install_dir = install_dir();

    // 2. Защита от прерванной установки (раздел VI плана): "грязная" папка
    //    от прошлой неудачной попытки удаляется полностью.
    if is_installation_dirty(&install_dir) {
        stage("Обнаружена незавершённая установка, очищаю...");
        tokio::fs::remove_dir_all(&install_dir).await.ok();
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
        download_with_resume(&client, &url, &dest).await?;

        let expected_sha = client.get(&sha_url).send().await?.text().await?;
        let ok = verify_sha256(&dest, expected_sha.trim()).await?;
        if !ok {
            anyhow::bail!("SHA256 не совпадает для {asset_name} — прерываю установку");
        }
    }

    // 5. Распаковка компонентов: server.zip — прямо в корень версии (даёт
    //    <version>/server.exe), launcher.zip — вне versions_dir, в
    //    install_dir/launcher (общий для всех версий), остальные — в свои
    //    подпапки внутри версии.
    stage("Распаковка компонентов...");
    for (zip_name, dest) in [
        ("server.zip", versions_dir.clone()),
        ("dist.zip", versions_dir.join("dist")),
        ("migrations.zip", versions_dir.join("migrations")),
        ("worker.zip", versions_dir.join("worker")),
        ("launcher.zip", install_dir.join("launcher")),
    ] {
        let zip_path = versions_dir.join(zip_name);
        tokio::task::spawn_blocking(move || extract_zip(&zip_path, &dest)).await??;
    }

    // 6. Инициализация PostgreSQL: фикс прав через icacls (раздел III/VI
    //    плана), генерация пароля, patch pg_hba.conf.
    stage("Настройка прав доступа к данным PostgreSQL...");
    let pg_data_dir = install_dir.join("pg16").join("data");
    tokio::fs::create_dir_all(&pg_data_dir).await?;
    restrict_to_current_user(&pg_data_dir).await?;

    stage("Генерация пароля базы данных...");
    let db_password = generate_password(24);
    let db_user = std::env::var("USERNAME").unwrap_or_else(|_| "evohime".to_string());
    let dsn = build_dsn(&db_user, &db_password, "127.0.0.1", 5432, "evohime");

    // Примечание: реальный вызов initdb/pg_ctl/createdb здесь опущен —
    // требует portable-архива PostgreSQL, который скачивается отдельным
    // шагом (POSTGRES_ARCHIVE_URL) и живо проверяется только на реальной
    // машине в Фазе 8. Точка расширения зафиксирована явно, а не спрятана.
    let pg_hba_path = pg_data_dir.join("pg_hba.conf");
    if pg_hba_path.exists() {
        stage("Настройка аутентификации PostgreSQL...");
        patch_pg_hba_trust_local(&pg_hba_path).await?;
    }

    // 7. Применение миграций программно (раздел III плана, без sqlx-cli).
    stage("Применение миграций базы данных...");
    let migrations_dir = versions_dir.join("migrations");
    if migrations_dir.exists() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&dsn)
            .await;
        if let Ok(pool) = pool {
            apply_migrations(&pool, &migrations_dir).await?;
        } else {
            stage("PostgreSQL пока не поднят — миграции будут применены при первом запуске Launcher'а.");
        }
    }

    // 8. Ярлык на рабочем столе (раздел VI плана).
    stage("Создание ярлыка...");
    let desktop_dir = dirs_desktop();
    let shortcut_path = desktop_dir.join("EvoHime Launcher.lnk");
    let launcher_exe = install_dir.join("launcher").join("evohime-launcher.exe");
    if let Some(parent) = shortcut_path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    let _ = create_shortcut(&shortcut_path, &launcher_exe).await;

    // 9. Финальный маркер — строго последним шагом (раздел VI плана).
    stage("Финализация установки...");
    mark_setup_complete(&install_dir).await?;

    let _ = tx.send(ProgressEvent::Done);
    Ok(())
}

fn dirs_desktop() -> PathBuf {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(user_profile).join("Desktop")
}
