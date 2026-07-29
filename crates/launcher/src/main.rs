//! EvoHime Launcher (evohime-launcher.exe) — Фазы 4-5 плана.
//!
//! Живёт в системном трее; запускает и мониторит `server.exe`/`worker.py`,
//! раздаёт React-статику (5173) и REST-статус (3001) на встроенных
//! axum-серверах, останавливает компоненты через `POST /shutdown` с
//! токеном сессии (раздел IV/XV плана). Проверяет обновления через
//! Atom-фид (не REST API — раздел V плана), применяет их только по клику
//! пользователя "Обновить сейчас", с атомарным переключением версии и
//! откатом при неудачном health-check.

use eframe::egui;
use evohime_launcher::config::{self, DbConfig};
use evohime_launcher::history::{append_and_save, UpdateHistoryEntry, UpdateOutcome};
use evohime_launcher::status_server::{ComponentStatus, LauncherStatus, StatusServerState};
use evohime_launcher::update_apply::{
    download_and_verify_assets, read_current_version, stage_and_switch, write_current_version,
    UpdatePlan,
};
use evohime_launcher::update_check::is_update_available;
use evohime_launcher::{
    build_dsn, build_static_router, build_status_router, build_updater_args, fetch_latest_release,
    generate_session_token, postgres, safe_mode, spawn_updater, ManagedProcess,
};
use evohime_win_support::SingleInstanceLock;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const STATIC_SERVER_PORT: u16 = 5173;
const STATUS_SERVER_PORT: u16 = 3001;
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);
const POST_UPDATE_HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const POST_UPDATE_HEALTH_ATTEMPTS: u32 = 5;
const GITHUB_REPO: &str = "rkfsociety/EvoHime";

#[derive(Default, Clone)]
struct UpdateState {
    available: bool,
    remote_version: Option<String>,
    in_progress: bool,
    last_message: String,
}

enum LauncherCommand {
    CheckUpdatesNow,
    ApplyUpdate,
    Stop,
    Restart,
}

fn main() -> eframe::Result<()> {
    let _instance_lock = match SingleInstanceLock::acquire("EvoHimeLauncherMutex") {
        Ok(lock) => lock,
        Err(err) => {
            eprintln!("EvoHime Launcher уже запущен: {err}");
            std::process::exit(1);
        }
    };

    let safe_mode = safe_mode::is_shift_held();
    if safe_mode {
        tracing::warn!("Safe Mode: запуск без проверки обновлений");
    }

    let session_token = generate_session_token();
    let install_dir = install_dir();
    let db_config = config::load(&install_dir);
    if db_config.is_none() {
        tracing::warn!(
            "launcher-data/config.json не найден — DATABASE_URL не будет передан server.exe"
        );
    }

    let runtime = tokio::runtime::Runtime::new().expect("failed to start Tokio runtime");
    let shared_status = Arc::new(Mutex::new(LauncherStatus {
        components: vec![
            ComponentStatus {
                name: "db".to_string(),
                online: false,
            },
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
    let update_state = Arc::new(Mutex::new(UpdateState::default()));

    let current_dir = current_version_dir(&install_dir);
    spawn_static_server(&runtime, current_dir.join("dist"), session_token.clone());
    spawn_status_server(&runtime, session_token.clone(), shared_status.clone());

    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    if !safe_mode {
        runtime.spawn(run_supervisor(
            install_dir,
            session_token,
            db_config,
            shared_status.clone(),
            update_state.clone(),
            command_rx,
        ));
    }

    let tray_menu = build_tray_menu();
    let tray_icon = tray_icon::TrayIconBuilder::new()
        .with_tooltip("EvoHime Launcher")
        .with_icon(status_icon(false))
        .with_menu(Box::new(tray_menu.menu))
        .build()
        .expect("failed to build tray icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([380.0, 260.0]),
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
                update_state,
                command_tx,
                safe_mode,
                show_confirm_dialog: false,
            }))
        }),
    )
}

/// Единая супервизор-задача: запускает `server.exe`, периодически
/// проверяет здоровье, периодически (Atom-фид) проверяет обновления,
/// обрабатывает команды из окна/трея ("Обновить сейчас", "Stop").
/// Полный atomic-update цикл (раздел V плана) с откатом при неудачном
/// health-check выполняется здесь же — управление процессом и логика
/// обновления должны быть в одном месте, иначе легко рассинхронизировать
/// "что сейчас реально запущено" при откате.
async fn run_supervisor(
    install_dir: PathBuf,
    token: String,
    db_config: Option<DbConfig>,
    status: Arc<Mutex<LauncherStatus>>,
    update_state: Arc<Mutex<UpdateState>>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<LauncherCommand>,
) {
    let client = reqwest::Client::new();
    let mut current_version = read_current_version(&install_dir);
    let workspace_dir = ensure_workspace_dir(&install_dir);

    let (pg_bin_dir, pg_data_dir) = pg_paths(&install_dir);
    let dsn = db_config.as_ref().map(|cfg| {
        build_dsn(
            &cfg.user,
            &cfg.password,
            "127.0.0.1",
            cfg.port,
            &cfg.db_name,
        )
    });

    if let Some(cfg) = &db_config {
        match postgres::ensure_started(&pg_bin_dir, &pg_data_dir, cfg.port).await {
            Ok(()) => update_component_status(&status, "db", true),
            Err(err) => tracing::error!(%err, "failed to start bundled PostgreSQL"),
        }
    }

    let mut server_process = start_server_process(
        &install_dir,
        &current_version,
        &workspace_dir,
        &token,
        dsn.as_deref(),
    )
    .await;
    let mut worker_process = start_worker_process(&install_dir, &current_version).await;

    let mut health_tick = tokio::time::interval(HEALTH_CHECK_INTERVAL);
    let mut update_tick = tokio::time::interval(UPDATE_CHECK_INTERVAL);

    loop {
        tokio::select! {
            _ = health_tick.tick() => {
                let online = server_process
                    .as_ref()
                    .map(|p| p.health_check(&client, Duration::from_secs(3)))
                    .into_iter()
                    .next();
                let online = if let Some(fut) = online { fut.await } else { false };
                update_component_status(&status, "server", online);

                let worker_online = match &worker_process {
                    Some(process) => process.health_check(&client, Duration::from_secs(3)).await,
                    None => false,
                };
                update_component_status(&status, "worker", worker_online);
            }
            _ = update_tick.tick() => {
                check_for_updates(&client, &current_version, &update_state, &status).await;
            }
            cmd = commands.recv() => {
                match cmd {
                    Some(LauncherCommand::CheckUpdatesNow) => {
                        check_for_updates(&client, &current_version, &update_state, &status).await;
                    }
                    Some(LauncherCommand::Stop) => {
                        if let Some(process) = &mut server_process {
                            process.graceful_shutdown(&client, &token, Duration::from_secs(10)).await;
                        }
                        update_component_status(&status, "server", false);

                        if let Some(process) = &mut worker_process {
                            process.graceful_shutdown(&client, &token, Duration::from_secs(5)).await;
                        }
                        update_component_status(&status, "worker", false);

                        if db_config.is_some() {
                            if let Err(err) = postgres::stop(&pg_bin_dir, &pg_data_dir).await {
                                tracing::error!(%err, "failed to stop bundled PostgreSQL");
                            }
                        }
                        update_component_status(&status, "db", false);
                    }
                    Some(LauncherCommand::Restart) => {
                        tracing::info!("restarting server/worker via tray command");
                        if let Some(mut process) = server_process.take() {
                            process
                                .graceful_shutdown(&client, &token, Duration::from_secs(10))
                                .await;
                        }
                        update_component_status(&status, "server", false);

                        if let Some(mut process) = worker_process.take() {
                            process
                                .graceful_shutdown(&client, &token, Duration::from_secs(5))
                                .await;
                        }
                        update_component_status(&status, "worker", false);

                        server_process = start_server_process(
                            &install_dir,
                            &current_version,
                            &workspace_dir,
                            &token,
                            dsn.as_deref(),
                        )
                        .await;
                        update_component_status(&status, "server", server_process.is_some());

                        worker_process =
                            start_worker_process(&install_dir, &current_version).await;
                        update_component_status(&status, "worker", worker_process.is_some());
                    }
                    Some(LauncherCommand::ApplyUpdate) => {
                        run_update_cycle(
                            &install_dir,
                            &workspace_dir,
                            &token,
                            dsn.as_deref(),
                            &client,
                            &mut current_version,
                            &mut server_process,
                            &mut worker_process,
                            &update_state,
                            &status,
                        )
                        .await;
                    }
                    None => break,
                }
            }
        }
    }
}

fn pg_paths(install_dir: &Path) -> (PathBuf, PathBuf) {
    let pg_dir = install_dir.join("pg16");
    (pg_dir.join("bin"), pg_dir.join("data"))
}

/// Переменные окружения, с которыми запускается `server.exe` — токен
/// сессии для локального shutdown-эндпоинта и (если БД уже развёрнута)
/// строка подключения, без которой сервер падает на несуществующий
/// дефолт (`crates/server/src/app.rs`).
fn server_env(token: &str, dsn: Option<&str>, workspace_root: &Path) -> Vec<(String, String)> {
    let mut env = vec![
        ("EVOHIME_LOCAL_TOKEN".to_string(), token.to_string()),
        (
            "WORKSPACE_ROOT".to_string(),
            workspace_root.display().to_string(),
        ),
    ];
    if let Some(dsn) = dsn {
        env.push(("DATABASE_URL".to_string(), dsn.to_string()));
    }
    env
}

#[cfg(test)]
mod server_env_tests {
    use super::*;

    #[test]
    fn passes_an_existing_workspace_root_to_the_server() {
        let root = Path::new(r"C:\EvoHime\versions\current");
        let env = server_env("token", Some("postgres://example"), root);

        assert!(env.contains(&("WORKSPACE_ROOT".to_string(), root.display().to_string())));
    }
}

async fn start_server_process(
    install_dir: &Path,
    version: &Option<String>,
    workspace_dir: &Path,
    token: &str,
    dsn: Option<&str>,
) -> Option<ManagedProcess> {
    let version = version.clone().unwrap_or_else(|| "current".to_string());
    let dir = install_dir.join("versions").join(version);
    let server_exe = dir.join("server.exe");
    if !server_exe.exists() {
        tracing::warn!(path = %server_exe.display(), "server.exe not found — nothing to supervise yet");
        return None;
    }

    let mut process = ManagedProcess::new(
        "server",
        server_exe,
        vec![],
        "http://127.0.0.1:3000/health",
        Some("http://127.0.0.1:3000/shutdown".to_string()),
    );
    if let Err(err) = process.start(&server_env(token, dsn, workspace_dir)).await {
        tracing::error!(%err, "failed to start server.exe");
        return None;
    }
    Some(process)
}

/// Каталог установки (`versions/<version>`) — это собственные бинарники
/// приложения, не рабочий проект пользователя: `git status`/`gh pr list`
/// там всегда падали (не git-репозиторий, нет remote). Заводим отдельную
/// стабильную по версиям директорию и делаем её валидным git-репозиторием,
/// чтобы git/GitHub-панели по умолчанию не начинали с ошибки 500.
fn ensure_workspace_dir(install_dir: &Path) -> PathBuf {
    let workspace = install_dir.join("workspace");
    if let Err(err) = std::fs::create_dir_all(&workspace) {
        tracing::error!(%err, path = %workspace.display(), "failed to create workspace directory");
        return workspace;
    }
    if !workspace.join(".git").exists() {
        match std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&workspace)
            .status()
        {
            Ok(status) if !status.success() => {
                tracing::warn!(%status, "git init in the workspace directory exited non-zero")
            }
            Err(err) => {
                tracing::warn!(%err, "failed to git-init the workspace directory — is git on PATH?")
            }
            Ok(_) => {}
        }
    }
    workspace
}

/// Запускает `worker.py` через системный Python (раздел II плана — worker
/// не компилируется в exe, раздаётся как чистый stdlib-скрипт). Порт 8090
/// зашит в `worker.py` по умолчанию и совпадает с дефолтом `PYTHON_WORKER_URL`
/// в `crates/server/src/app.rs`, так что явно его не передаём.
async fn start_worker_process(
    install_dir: &Path,
    version: &Option<String>,
) -> Option<ManagedProcess> {
    let version = version.clone().unwrap_or_else(|| "current".to_string());
    let dir = install_dir.join("versions").join(version);
    let worker_script = dir.join("worker").join("worker.py");
    if !worker_script.exists() {
        tracing::warn!(path = %worker_script.display(), "worker.py not found — nothing to supervise yet");
        return None;
    }

    let mut process = ManagedProcess::new(
        "worker",
        PathBuf::from("python"),
        vec![worker_script.display().to_string()],
        "http://127.0.0.1:8090/health",
        None,
    );
    if let Err(err) = process.start(&[]).await {
        tracing::error!(%err, "failed to start worker.py — is Python installed and on PATH?");
        return None;
    }
    Some(process)
}

fn update_component_status(status: &Arc<Mutex<LauncherStatus>>, name: &str, online: bool) {
    if let Ok(mut guard) = status.lock() {
        if let Some(component) = guard.components.iter_mut().find(|c| c.name == name) {
            component.online = online;
        }
    }
}

async fn check_for_updates(
    client: &reqwest::Client,
    current_version: &Option<String>,
    update_state: &Arc<Mutex<UpdateState>>,
    status: &Arc<Mutex<LauncherStatus>>,
) {
    let Ok((remote_version, assets, _launcher_asset)) =
        fetch_latest_release(client, GITHUB_REPO).await
    else {
        tracing::warn!("failed to fetch the latest GitHub release — skipping this check");
        return;
    };
    if assets.len() != 4 {
        tracing::warn!(
            asset_count = assets.len(),
            "latest release is incomplete — skipping this check"
        );
        return;
    }

    let local = current_version.clone().unwrap_or_default();
    let available = is_update_available(&local, &remote_version);

    if let Ok(mut state) = update_state.lock() {
        state.available = available;
        state.remote_version = Some(remote_version);
    }
    if let Ok(mut guard) = status.lock() {
        guard.update_available = available;
    }
}

/// Полный atomic-update цикл (раздел V плана): REST API (только здесь) →
/// скачивание+SHA256 → распаковка во `_tmp` → миграции → атомарный rename
/// → graceful shutdown старого процесса → запуск нового → health-check с
/// адаптивным таймаутом → откат на предыдущую версию при неудаче.
#[allow(clippy::too_many_arguments)]
async fn run_update_cycle(
    install_dir: &Path,
    workspace_dir: &Path,
    token: &str,
    dsn: Option<&str>,
    client: &reqwest::Client,
    current_version: &mut Option<String>,
    server_process: &mut Option<ManagedProcess>,
    worker_process: &mut Option<ManagedProcess>,
    update_state: &Arc<Mutex<UpdateState>>,
    status: &Arc<Mutex<LauncherStatus>>,
) {
    set_in_progress(update_state, true, "Получение списка ассетов релиза...");

    let saved_version = current_version.clone();

    let (new_version, assets, launcher_asset) =
        match fetch_latest_release(client, GITHUB_REPO).await {
            Ok(result) => result,
            Err(err) => {
                finish_update(
                    update_state,
                    status,
                    false,
                    format!("Не удалось получить релиз: {err}"),
                );
                return;
            }
        };

    let plan = UpdatePlan {
        install_dir: install_dir.to_path_buf(),
        new_version: new_version.clone(),
        assets,
        dsn: dsn.map(|s| s.to_string()),
    };
    let download_dir = install_dir.join("launcher-data").join("download_tmp");
    let progress = |msg: &str| set_in_progress(update_state, true, msg);

    if let Err(err) = download_and_verify_assets(&plan, client, &download_dir, &progress).await {
        rollback_and_record(
            update_state,
            status,
            install_dir,
            saved_version.as_deref(),
            &format!("Обновление отменено: {err}"),
        )
        .await;
        return;
    }

    let final_dir = match stage_and_switch(&plan, &download_dir, &progress).await {
        Ok(dir) => dir,
        Err(err) => {
            rollback_and_record(
                update_state,
                status,
                install_dir,
                saved_version.as_deref(),
                &format!("Обновление отменено: {err}"),
            )
            .await;
            return;
        }
    };

    if let Err(err) = write_current_version(install_dir, &new_version).await {
        rollback_and_record(
            update_state,
            status,
            install_dir,
            saved_version.as_deref(),
            &format!("Не удалось переключить версию: {err}"),
        )
        .await;
        return;
    }

    progress("Остановка предыдущей версии...");
    if let Some(process) = server_process.take() {
        let mut process = process;
        process
            .graceful_shutdown(client, token, Duration::from_secs(10))
            .await;
    }
    if let Some(process) = worker_process.take() {
        let mut process = process;
        process
            .graceful_shutdown(client, token, Duration::from_secs(5))
            .await;
    }
    update_component_status(status, "worker", false);

    progress("Запуск новой версии...");
    let new_server_exe = final_dir.join("server.exe");
    let mut new_process = ManagedProcess::new(
        "server",
        new_server_exe,
        vec![],
        "http://127.0.0.1:3000/health",
        Some("http://127.0.0.1:3000/shutdown".to_string()),
    );
    let start_ok = new_process
        .start(&server_env(token, dsn, workspace_dir))
        .await
        .is_ok();

    let healthy = start_ok
        && wait_for_health(
            &new_process,
            client,
            POST_UPDATE_HEALTH_TIMEOUT,
            POST_UPDATE_HEALTH_ATTEMPTS,
        )
        .await;

    if !healthy {
        new_process
            .graceful_shutdown(client, token, Duration::from_secs(5))
            .await;
        rollback_and_record(
            update_state,
            status,
            install_dir,
            saved_version.as_deref(),
            "Health-check после обновления не прошёл",
        )
        .await;
        *server_process =
            start_server_process(install_dir, &saved_version, workspace_dir, token, dsn).await;
        update_component_status(status, "server", server_process.is_some());
        *worker_process = start_worker_process(install_dir, &saved_version).await;
        update_component_status(status, "worker", worker_process.is_some());
        return;
    }

    *server_process = Some(new_process);
    *current_version = Some(new_version.clone());
    update_component_status(status, "server", true);
    *worker_process = start_worker_process(install_dir, current_version).await;
    update_component_status(status, "worker", worker_process.is_some());

    let entry = UpdateHistoryEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        from_version: saved_version.clone().unwrap_or_default(),
        to_version: new_version,
        outcome: UpdateOutcome::Success,
        message: String::new(),
    };
    let history_path = install_dir.join("launcher-data").join("history.json");
    let _ = append_and_save(&history_path, entry).await;

    finish_update(update_state, status, true, "Готово!".to_string());

    // Отдельный, необязательный шаг: launcher.zip не входит в основной
    // 4-ассетный цикл выше (см. комментарий у `LAUNCHER_ASSET` в
    // github_api.rs), поэтому докачивается и применяется здесь же, но
    // изолированно — сбой здесь не должен откатывать уже успешно
    // применённое обновление server/dist/migrations/worker.
    if let Some(launcher_asset) = launcher_asset {
        let launcher_plan = UpdatePlan {
            install_dir: install_dir.to_path_buf(),
            new_version: "launcher".to_string(),
            assets: vec![launcher_asset],
            dsn: None,
        };
        if let Err(err) =
            download_and_verify_assets(&launcher_plan, client, &download_dir, &progress).await
        {
            tracing::warn!(%err, "failed to download launcher.zip — launcher self-update skipped this cycle");
        } else {
            apply_launcher_self_update(
                install_dir,
                &download_dir,
                token,
                client,
                server_process,
                worker_process,
                &progress,
            )
            .await;
        }
    }
}

/// Отдельное, best-effort самообновление `evohime-launcher.exe` (раздел
/// VIII плана). В отличие от server/dist/migrations/worker, launcher не
/// может подменить собственный работающий файл — вместо этого передаёт
/// эстафету внешнему `updater.exe`: тот дожидается завершения текущего
/// процесса, атомарно заменяет exe и перезапускает уже новую версию.
/// При успехе управление из этой функции не возвращается — процесс
/// завершается сам (`std::process::exit`), чтобы освободить файл для
/// `updater.exe`. Любая ошибка на пути — просто предупреждение в лог:
/// старый launcher продолжает работать с уже обновлёнными server/worker,
/// а самообновление лаунчера попробуется снова в следующем цикле.
#[allow(clippy::too_many_arguments)]
async fn apply_launcher_self_update(
    install_dir: &Path,
    download_dir: &Path,
    token: &str,
    client: &reqwest::Client,
    server_process: &mut Option<ManagedProcess>,
    worker_process: &mut Option<ManagedProcess>,
    progress: &(dyn Fn(&str) + Send + Sync),
) {
    let launcher_zip = download_dir.join("launcher.zip");
    if !launcher_zip.exists() {
        return;
    }

    progress("Обновление Launcher'а...");
    let extract_dir = install_dir
        .join("launcher-data")
        .join("launcher_update_tmp");
    if extract_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&extract_dir).await;
    }
    if let Err(err) = tokio::fs::create_dir_all(&extract_dir).await {
        tracing::warn!(%err, "failed to prepare the launcher update directory — skipping");
        return;
    }

    let extract_task = {
        let launcher_zip = launcher_zip.clone();
        let extract_dir = extract_dir.clone();
        tokio::task::spawn_blocking(move || {
            evohime_artifacts::extract_zip(&launcher_zip, &extract_dir)
        })
    };
    match extract_task.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::warn!(%err, "failed to extract launcher.zip — skipping launcher self-update");
            return;
        }
        Err(err) => {
            tracing::warn!(%err, "launcher.zip extraction task panicked — skipping launcher self-update");
            return;
        }
    }

    let new_launcher_exe = extract_dir.join("evohime-launcher.exe");
    let new_updater_exe = extract_dir.join("updater.exe");
    if !new_launcher_exe.exists() || !new_updater_exe.exists() {
        tracing::warn!(
            "launcher.zip did not contain the expected evohime-launcher.exe/updater.exe — skipping"
        );
        return;
    }

    // updater.exe должен пережить выход текущего процесса, поэтому не
    // может остаться во временной папке загрузки, которую следующий цикл
    // обновления вправе удалить или перезаписать.
    let updater_dest = install_dir.join("launcher-data").join("updater.exe");
    if let Err(err) = tokio::fs::copy(&new_updater_exe, &updater_dest).await {
        tracing::warn!(%err, "failed to stage updater.exe — skipping launcher self-update");
        return;
    }

    let own_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!(%err, "failed to resolve own executable path — skipping launcher self-update");
            return;
        }
    };
    let own_pid = std::process::id();
    let updater_args = match build_updater_args(own_exe, own_pid, new_launcher_exe) {
        Ok(args) => args,
        Err(err) => {
            tracing::warn!(%err, "failed to build updater arguments — skipping launcher self-update");
            return;
        }
    };

    progress("Перезапуск Launcher'а...");
    if let Some(mut process) = server_process.take() {
        process
            .graceful_shutdown(client, token, Duration::from_secs(10))
            .await;
    }
    if let Some(mut process) = worker_process.take() {
        process
            .graceful_shutdown(client, token, Duration::from_secs(5))
            .await;
    }

    if let Err(err) = spawn_updater(&updater_dest, &updater_args) {
        tracing::warn!(%err, "failed to spawn updater.exe — launcher self-update aborted, this session keeps running");
        return;
    }

    // updater.exe теперь ждёт завершения именно этого процесса (по PID +
    // exe-пути + времени старта), прежде чем сможет безопасно заменить
    // файл. Возврат отсюда без выхода означает, что updater.exe будет
    // ждать полные 30 секунд и затем убьёт нас принудительно — выходим
    // сразу и чисто.
    std::process::exit(0);
}

#[cfg(test)]
mod launcher_self_update_tests {
    /// Проверяет, что `launcher.zip` в том же формате, что публикует
    /// `build-releases.yml` (`evohime-launcher.exe`+`updater.exe` на корне
    /// архива, без вложенной папки — см. `Compress-Archive -Path
    /// artifact/evohime-launcher.exe,artifact/updater.exe`), распаковывается
    /// ровно туда, где их ищет `apply_launcher_self_update`
    /// (`extract_dir.join("evohime-launcher.exe")` / `.join("updater.exe")`).
    #[test]
    fn launcher_zip_extracts_both_exes_to_the_expected_flat_paths() {
        let zip_dir = tempfile::tempdir().unwrap();
        let zip_path = zip_dir.path().join("launcher.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            use std::io::Write;
            writer.start_file("evohime-launcher.exe", options).unwrap();
            writer.write_all(b"fake launcher bytes").unwrap();
            writer.start_file("updater.exe", options).unwrap();
            writer.write_all(b"fake updater bytes").unwrap();
            writer.finish().unwrap();
        }

        let extract_dir = tempfile::tempdir().unwrap();
        evohime_artifacts::extract_zip(&zip_path, extract_dir.path()).unwrap();

        assert!(extract_dir.path().join("evohime-launcher.exe").exists());
        assert!(extract_dir.path().join("updater.exe").exists());
    }
}

async fn wait_for_health(
    process: &ManagedProcess,
    client: &reqwest::Client,
    total_timeout: Duration,
    attempts: u32,
) -> bool {
    let per_attempt = total_timeout / attempts.max(1);
    for _ in 0..attempts {
        if process.health_check(client, per_attempt).await {
            return true;
        }
        tokio::time::sleep(per_attempt).await;
    }
    false
}

async fn rollback_and_record(
    update_state: &Arc<Mutex<UpdateState>>,
    status: &Arc<Mutex<LauncherStatus>>,
    install_dir: &Path,
    saved_version: Option<&str>,
    message: &str,
) {
    if let Some(version) = saved_version {
        let _ = write_current_version(install_dir, version).await;
    }
    let entry = UpdateHistoryEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        from_version: saved_version.unwrap_or_default().to_string(),
        to_version: saved_version.unwrap_or_default().to_string(),
        outcome: UpdateOutcome::RolledBack,
        message: message.to_string(),
    };
    let history_path = install_dir.join("launcher-data").join("history.json");
    let _ = append_and_save(&history_path, entry).await;

    finish_update(update_state, status, false, message.to_string());
}

fn set_in_progress(update_state: &Arc<Mutex<UpdateState>>, in_progress: bool, message: &str) {
    if let Ok(mut state) = update_state.lock() {
        state.in_progress = in_progress;
        state.last_message = message.to_string();
    }
}

fn finish_update(
    update_state: &Arc<Mutex<UpdateState>>,
    status: &Arc<Mutex<LauncherStatus>>,
    success: bool,
    message: String,
) {
    if let Ok(mut state) = update_state.lock() {
        state.in_progress = false;
        state.available = !success; // при неудаче обновление всё ещё "доступно"
        state.last_message = message;
    }
    if let Ok(mut guard) = status.lock() {
        guard.update_available = !success;
    }
}

fn install_dir() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data).join("EvoHime")
}

fn current_version_dir(install_dir: &Path) -> PathBuf {
    let version = read_current_version(install_dir).unwrap_or_else(|| "current".to_string());
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

struct TrayMenu {
    menu: tray_icon::menu::Menu,
    ids: TrayMenuIds,
}

struct TrayMenuIds {
    open_dashboard: tray_icon::menu::MenuId,
    check_updates: tray_icon::menu::MenuId,
    stop: tray_icon::menu::MenuId,
    restart: tray_icon::menu::MenuId,
    exit: tray_icon::menu::MenuId,
}

fn build_tray_menu() -> TrayMenu {
    let menu = tray_icon::menu::Menu::new();

    let open_dashboard = tray_icon::menu::MenuItem::new("Open Dashboard", true, None);
    let check_updates = tray_icon::menu::MenuItem::new("Check Updates", true, None);
    let stop = tray_icon::menu::MenuItem::new("Stop", true, None);
    let restart = tray_icon::menu::MenuItem::new("Restart", true, None);
    let settings = tray_icon::menu::MenuItem::new("Settings", true, None);
    let exit = tray_icon::menu::MenuItem::new("Exit", true, None);

    let ids = TrayMenuIds {
        open_dashboard: open_dashboard.id().clone(),
        check_updates: check_updates.id().clone(),
        stop: stop.id().clone(),
        restart: restart.id().clone(),
        exit: exit.id().clone(),
    };

    // Settings полноценно заработает в Фазе 7 (веб-панель/окно настроек) —
    // здесь присутствует в меню (раздел VII плана), но пока без обработчика.
    menu.append_items(&[
        &open_dashboard,
        &check_updates,
        &stop,
        &restart,
        &settings,
        &tray_icon::menu::PredefinedMenuItem::separator(),
        &exit,
    ])
    .expect("failed to append tray menu items");

    TrayMenu { menu, ids }
}

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

struct LauncherApp {
    _runtime: tokio::runtime::Runtime,
    _instance_lock: SingleInstanceLock,
    _tray_icon: tray_icon::TrayIcon,
    tray_ids: TrayMenuIds,
    status: Arc<Mutex<LauncherStatus>>,
    update_state: Arc<Mutex<UpdateState>>,
    command_tx: tokio::sync::mpsc::UnboundedSender<LauncherCommand>,
    safe_mode: bool,
    show_confirm_dialog: bool,
}

impl eframe::App for LauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::DoubleClick { .. } = event {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        // Раздел II плана: закрытие крестиком сворачивает в трей вместо
        // выхода — приложение продолжает раздавать 5173/3000/3001, пока
        // пользователь явно не выберет "Exit" в меню трея.
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == self.tray_ids.open_dashboard {
                let _ = webbrowser_open("http://localhost:5173");
            } else if event.id == self.tray_ids.check_updates {
                let _ = self.command_tx.send(LauncherCommand::CheckUpdatesNow);
            } else if event.id == self.tray_ids.stop {
                let _ = self.command_tx.send(LauncherCommand::Stop);
            } else if event.id == self.tray_ids.restart {
                let _ = self.command_tx.send(LauncherCommand::Restart);
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
        if ui.button("Проверить обновления").clicked() {
            let _ = self.command_tx.send(LauncherCommand::CheckUpdatesNow);
        }

        let (available, in_progress, remote_version, message) = self
            .update_state
            .lock()
            .map(|s| {
                (
                    s.available,
                    s.in_progress,
                    s.remote_version.clone(),
                    s.last_message.clone(),
                )
            })
            .unwrap_or((false, false, None, String::new()));

        if in_progress {
            ui.separator();
            ui.spinner();
            ui.label(&message);
        } else if available {
            ui.separator();
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "🔴 Доступно обновление: {}",
                    remote_version.unwrap_or_default()
                ),
            );
            if ui.button("Обновить сейчас").clicked() {
                self.show_confirm_dialog = true;
            }
        } else if !message.is_empty() {
            ui.separator();
            ui.label(&message);
        }

        if self.show_confirm_dialog {
            egui::Window::new("Подтвердите обновление")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label("Приложение будет перезапущено. Текущие сессии прервутся.");
                    ui.horizontal(|ui| {
                        if ui.button("Обновить сейчас").clicked() {
                            let _ = self.command_tx.send(LauncherCommand::ApplyUpdate);
                            self.show_confirm_dialog = false;
                        }
                        if ui.button("Отмена").clicked() {
                            self.show_confirm_dialog = false;
                        }
                    });
                });
        }

        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

fn webbrowser_open(url: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn()
}
