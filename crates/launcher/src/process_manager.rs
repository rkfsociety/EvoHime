//! Управление дочерними процессами (`server.exe`, `worker.py`): запуск,
//! health-check, graceful shutdown через `POST /shutdown` с токеном
//! (раздел IV/VII плана) — не через `CTRL_C_EVENT`, который на Windows
//! нестабилен для процессов без общей консоли с родителем.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

pub struct ManagedProcess {
    pub name: String,
    exe: PathBuf,
    args: Vec<String>,
    pub health_url: String,
    /// `None` для процессов без собственного HTTP `/shutdown` (в этом
    /// случае используется только принудительное завершение).
    pub shutdown_url: Option<String>,
    child: Option<Child>,
}

impl ManagedProcess {
    pub fn new(
        name: impl Into<String>,
        exe: PathBuf,
        args: Vec<String>,
        health_url: impl Into<String>,
        shutdown_url: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            exe,
            args,
            health_url: health_url.into(),
            shutdown_url,
            child: None,
        }
    }

    /// Запускает процесс с дополнительными переменными окружения (в первую
    /// очередь `EVOHIME_LOCAL_TOKEN` — раздел XV плана).
    pub async fn start(&mut self, extra_env: &[(String, String)]) -> std::io::Result<()> {
        let mut cmd = Command::new(&self.exe);
        cmd.args(&self.args)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        let child = cmd.spawn()?;
        self.child = Some(child);
        Ok(())
    }

    /// `true`, если процесс всё ещё выполняется (не завершился сам и не
    /// был убит).
    pub fn is_running(&mut self) -> bool {
        match &mut self.child {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Опрашивает `health_url` с таймаутом; `false` при любой ошибке
    /// (не только "не 200") — недоступность так же плоха, как явный сбой.
    pub async fn health_check(&self, client: &reqwest::Client, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, client.get(&self.health_url).send())
            .await
            .ok()
            .and_then(|result| result.ok())
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    /// Останавливает процесс: сначала пробует `POST /shutdown` с токеном
    /// (если у процесса есть `shutdown_url`) и ждёт штатного завершения,
    /// затем принудительно убивает, если процесс не завершился в течение
    /// `timeout`.
    pub async fn graceful_shutdown(
        &mut self,
        client: &reqwest::Client,
        token: &str,
        timeout: Duration,
    ) {
        if let Some(url) = &self.shutdown_url {
            let _ = client.post(url).bearer_auth(token).send().await;
        }

        if let Some(child) = &mut self.child {
            if tokio::time::timeout(timeout, child.wait()).await.is_err() {
                let _ = child.kill().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    async fn spawn_test_server(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    /// Использует `ping` (не `timeout`) как долгоживущий процесс-заглушку:
    /// в некоторых окружениях `PATH` перед системным
    /// `C:\Windows\System32\timeout.exe` оказывается версия `timeout` из
    /// Git for Windows (coreutils) с другим синтаксисом аргументов — она
    /// немедленно завершается с ошибкой вместо ожидания, что незаметно
    /// делает тест "проходящим" по неверной причине (процесс уже мёртв
    /// до истечения таймаута graceful_shutdown, а не после). `ping` не
    /// имеет такого неоднозначного тёзки.
    fn windows_sleep_process(seconds: u32) -> (PathBuf, Vec<String>) {
        (
            PathBuf::from("cmd"),
            vec![
                "/c".to_string(),
                format!("ping 127.0.0.1 -n {} > nul", seconds + 1),
            ],
        )
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn spawned_process_reports_running_then_stopping() {
        let (exe, args) = windows_sleep_process(5);
        let mut process = ManagedProcess::new("test-sleep", exe, args, "http://unused", None);

        process.start(&[]).await.expect("spawn should succeed");
        assert!(
            process.is_running(),
            "freshly spawned process should be running"
        );

        process
            .graceful_shutdown(
                &reqwest::Client::new(),
                "unused-token",
                Duration::from_secs(2),
            )
            .await;
        assert!(
            !process.is_running(),
            "process should be stopped after graceful_shutdown's force-kill fallback"
        );
    }

    #[tokio::test]
    async fn health_check_true_when_endpoint_responds_ok() {
        let router = Router::new().route("/health", get(|| async { StatusCode::OK }));
        let base_url = spawn_test_server(router).await;

        let process = ManagedProcess::new(
            "fake-server",
            PathBuf::from("unused"),
            vec![],
            format!("{base_url}/health"),
            None,
        );

        let client = reqwest::Client::new();
        assert!(process.health_check(&client, Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn health_check_false_when_nothing_listening() {
        let process = ManagedProcess::new(
            "fake-server",
            PathBuf::from("unused"),
            vec![],
            "http://127.0.0.1:1", // reserved/unused port, connection should fail fast
            None,
        );

        let client = reqwest::Client::new();
        assert!(!process.health_check(&client, Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn graceful_shutdown_posts_with_correct_bearer_token() {
        let received_correct_token = Arc::new(AtomicBool::new(false));
        let flag_for_handler = received_correct_token.clone();

        async fn shutdown_handler(
            State(flag): State<Arc<AtomicBool>>,
            headers: HeaderMap,
        ) -> StatusCode {
            let ok = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                == Some("Bearer expected-token");
            flag.store(ok, Ordering::SeqCst);
            StatusCode::OK
        }

        let router = Router::new()
            .route("/shutdown", post(shutdown_handler))
            .with_state(flag_for_handler);
        let base_url = spawn_test_server(router).await;

        let mut process = ManagedProcess::new(
            "fake-server",
            PathBuf::from("unused"),
            vec![],
            "http://unused",
            Some(format!("{base_url}/shutdown")),
        );

        process
            .graceful_shutdown(
                &reqwest::Client::new(),
                "expected-token",
                Duration::from_secs(1),
            )
            .await;

        assert!(
            received_correct_token.load(Ordering::SeqCst),
            "shutdown endpoint should have received the correct bearer token"
        );
    }
}
