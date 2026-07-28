# Installer Fresh Release Downloads Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сделать так, чтобы `evohime-setup.exe` при каждом запуске заново скачивал и проверял все архивы `releases/latest`, не используя HTTP Range и остатки предыдущей попытки.

**Architecture:** В `evohime-artifacts` появится отдельная операция свежей загрузки: она получает новый checksum, удаляет старые окончательный файл и `.part`, скачивает архив целиком во временный файл, проверяет SHA256 и только затем переименовывает его. Установщик будет использовать только эту операцию; существующая проверяемая докачка лаунчера останется без изменений. Очистка каталога незавершённой установки станет обязательной и перестанет игнорировать ошибки Windows.

**Tech Stack:** Rust 2021, Tokio async filesystem/I/O, Reqwest, Axum test server, SHA256 из `evohime-artifacts`, Windows `OpenOptionsExt`.

## Global Constraints

- Изменяется только первоначальная установка в `crates/installer`; механизм обновления в `crates/launcher` сохраняет Range-докачку.
- Установщик продолжает использовать `https://github.com/rkfsociety/EvoHime/releases/latest/download/<asset>`.
- Каждый запуск заново загружает `server.zip`, `launcher.zip`, `dist.zip`, `migrations.zip`, `worker.zip`, `postgres.zip` и соответствующие `.sha256`.
- Свежая загрузка никогда не отправляет HTTP `Range`.
- Окончательный файл появляется только после успешной проверки SHA256 временного `<asset>.part`.
- Отсутствие старого файла допустимо; любая другая ошибка удаления останавливает установку.
- Ошибка очистки каталога без `.setup_complete` должна останавливать установку.
- Не создавать ветку или worktree; работать в текущей `main`.
- После каждого законченного изменения создавать отдельный относящийся к задаче коммит; не пушить без отдельной команды пользователя.
- После финальной проверки выполнить `cargo clean`.

---

### Task 1: Свежая проверяемая загрузка без Range

**Files:**
- Modify: `crates/artifacts/src/downloader.rs:6-208`
- Modify: `crates/artifacts/src/lib.rs:8-14`

**Interfaces:**
- Consumes: `crate::sha256::verify_sha256(path: &Path, expected_hex: &str) -> std::io::Result<bool>`
- Produces: `download_fresh_and_verify(client: &reqwest::Client, url: &str, sha256_url: &str, dest: &Path) -> Result<bool, DownloadError>`
- Preserves: `download_with_resume` and `download_with_resume_and_verify` without contract changes

- [ ] **Step 1: Write failing tests for fresh replacement and `.part` cleanup**

In `crates/artifacts/src/downloader.rs`, extend the test imports and add a server that rejects every asset request carrying `Range`:

```rust
use axum::{
    body::Bytes,
    http::{header::RANGE, HeaderMap},
    response::IntoResponse,
    routing::get,
    Router,
};

const FRESH_CONTENT: &[u8] = b"fresh artifact";
const FRESH_SHA256: &str =
    "fba70a783cecd8de271f147d7afabee99f3ee796d97a080293f0adc2fbfff0af";

async fn spawn_fresh_download_server(checksum: &'static str) -> String {
    let app = Router::new()
        .route(
            "/artifact.bin",
            get(|headers: HeaderMap| async move {
                if headers.contains_key(RANGE) {
                    return StatusCode::BAD_REQUEST.into_response();
                }
                (StatusCode::OK, Bytes::from_static(FRESH_CONTENT)).into_response()
            }),
        )
        .route(
            "/artifact.bin.sha256",
            get(move || async move { (StatusCode::OK, checksum) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn fresh_download_replaces_old_files_without_range() {
    let base_url = spawn_fresh_download_server(FRESH_SHA256).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("artifact.bin");
    let part = dir.path().join("artifact.bin.part");
    tokio::fs::write(&dest, b"old release").await.unwrap();
    tokio::fs::write(&part, b"interrupted old release")
        .await
        .unwrap();

    let ok = download_fresh_and_verify(
        &reqwest::Client::new(),
        &format!("{base_url}/artifact.bin"),
        &format!("{base_url}/artifact.bin.sha256"),
        &dest,
    )
    .await
    .unwrap();

    assert!(ok);
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), FRESH_CONTENT);
    assert!(!part.exists());
}

#[tokio::test]
async fn fresh_download_removes_part_when_checksum_is_wrong() {
    let bad_sha = "0000000000000000000000000000000000000000000000000000000000000000";
    let base_url = spawn_fresh_download_server(bad_sha).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("artifact.bin");
    let part = dir.path().join("artifact.bin.part");
    tokio::fs::write(&dest, b"old release").await.unwrap();

    let ok = download_fresh_and_verify(
        &reqwest::Client::new(),
        &format!("{base_url}/artifact.bin"),
        &format!("{base_url}/artifact.bin.sha256"),
        &dest,
    )
    .await
    .unwrap();

    assert!(!ok);
    assert!(!dest.exists());
    assert!(!part.exists());
}

#[cfg(windows)]
#[tokio::test]
async fn fresh_download_propagates_failure_to_remove_old_file() {
    use std::os::windows::fs::OpenOptionsExt;

    let base_url = spawn_fresh_download_server(FRESH_SHA256).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("artifact.bin");
    let locked = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0)
        .open(&dest)
        .unwrap();

    let result = download_fresh_and_verify(
        &reqwest::Client::new(),
        &format!("{base_url}/artifact.bin"),
        &format!("{base_url}/artifact.bin.sha256"),
        &dest,
    )
    .await;

    assert!(matches!(result, Err(DownloadError::Io(_))));
    drop(locked);
}
```

The production change caught by these tests is any implementation that reuses `dest`, sends `Range`, exposes `.part` as the final file, or retains a checksum-mismatched download.

- [ ] **Step 2: Run the tests and verify RED**

Run:

```powershell
cargo test -p evohime-artifacts fresh_download_ -- --nocapture
```

Expected: compilation fails because `download_fresh_and_verify` does not exist. Do not add production code until this failure is observed.

- [ ] **Step 3: Implement the minimal fresh-download operation**

In `crates/artifacts/src/downloader.rs`, import `PathBuf` and add these helpers after `download_with_resume_and_verify`:

```rust
fn part_path(dest: &Path) -> PathBuf {
    let mut value = dest.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

async fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub async fn download_fresh_and_verify(
    client: &reqwest::Client,
    url: &str,
    sha256_url: &str,
    dest: &Path,
) -> Result<bool, DownloadError> {
    let expected_sha256 = client
        .get(sha256_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let part = part_path(dest);
    remove_file_if_exists(dest).await?;
    remove_file_if_exists(&part).await?;

    let download_result = async {
        let response = client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(DownloadError::HttpStatus(status.as_u16()));
        }

        let mut file = File::create(&part).await?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }
        file.flush().await?;
        Ok::<(), DownloadError>(())
    }
    .await;

    if let Err(err) = download_result {
        let _ = remove_file_if_exists(&part).await;
        return Err(err);
    }

    let verified = match crate::sha256::verify_sha256(&part, expected_sha256.trim()).await {
        Ok(value) => value,
        Err(err) => {
            let _ = remove_file_if_exists(&part).await;
            return Err(err.into());
        }
    };
    if !verified {
        remove_file_if_exists(&part).await?;
        return Ok(false);
    }

    tokio::fs::rename(&part, dest).await?;
    Ok(true)
}
```

Update the import:

```rust
use std::path::{Path, PathBuf};
```

Export the function from `crates/artifacts/src/lib.rs`:

```rust
pub use downloader::{
    download_fresh_and_verify, download_with_resume, download_with_resume_and_verify, DownloadError,
};
```

- [ ] **Step 4: Run focused and crate tests**

Run:

```powershell
cargo test -p evohime-artifacts fresh_download_ -- --nocapture
cargo test -p evohime-artifacts
```

Expected: both focused tests pass; the complete artifacts crate reports zero failed tests.

- [ ] **Step 5: Commit Task 1**

```powershell
git add -- crates/artifacts/src/downloader.rs crates/artifacts/src/lib.rs
git commit -m "feat(installer): add fresh verified downloads"
```

---

### Task 2: Обязательная очистка незавершённой установки

**Files:**
- Modify: `crates/installer/src/setup_marker.rs:10-65`
- Modify: `crates/installer/src/lib.rs:12-19`
- Modify: `crates/installer/src/main.rs:11-15,265-273`
- Create: `crates/installer/tests/setup_cleanup_windows.rs`

**Interfaces:**
- Consumes: `is_installation_dirty(install_dir: &Path) -> bool`
- Produces: `clear_dirty_installation(install_dir: &Path) -> std::io::Result<bool>`
- Return value: `true` only when a dirty directory existed and was removed; `false` when no cleanup was required

- [ ] **Step 1: Write a Windows integration test proving cleanup errors propagate**

Create `crates/installer/tests/setup_cleanup_windows.rs`:

```rust
#![cfg(windows)]

use evohime_installer::clear_dirty_installation;
use std::os::windows::fs::OpenOptionsExt;

#[tokio::test]
async fn removes_dirty_installation_before_continuing() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    tokio::fs::create_dir_all(&install_dir).await.unwrap();
    tokio::fs::write(install_dir.join("partial.bin"), b"partial")
        .await
        .unwrap();

    assert!(clear_dirty_installation(&install_dir).await.unwrap());
    assert!(!install_dir.exists());
}

#[tokio::test]
async fn reports_error_when_dirty_installation_cannot_be_removed() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    tokio::fs::create_dir_all(&install_dir).await.unwrap();
    let locked_path = install_dir.join("locked.bin");
    let locked = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0)
        .open(&locked_path)
        .unwrap();

    let result = clear_dirty_installation(&install_dir).await;

    assert!(result.is_err());
    assert!(install_dir.exists());
    drop(locked);
}
```

The production change caught by the second test is replacing the propagated `remove_dir_all` error with `.ok()` or otherwise continuing after Windows denies deletion.

- [ ] **Step 2: Run the integration target and verify RED**

Run:

```powershell
cargo test -p evohime-installer --test setup_cleanup_windows -- --nocapture
```

Expected: compilation fails because `clear_dirty_installation` is not exported.

- [ ] **Step 3: Implement and export strict cleanup**

Add to `crates/installer/src/setup_marker.rs`:

```rust
pub async fn clear_dirty_installation(install_dir: &Path) -> std::io::Result<bool> {
    if !is_installation_dirty(install_dir) {
        return Ok(false);
    }
    tokio::fs::remove_dir_all(install_dir).await?;
    Ok(true)
}
```

Update `crates/installer/src/lib.rs`:

```rust
pub use setup_marker::{
    clear_dirty_installation, is_installation_dirty, mark_setup_complete,
};
```

Update the installer import in `crates/installer/src/main.rs` to include `clear_dirty_installation`. Replace the ignored removal with:

```rust
if is_installation_dirty(&install_dir) {
    stage("Обнаружена незавершённая установка, очищаю...");
    clear_dirty_installation(&install_dir)
        .await
        .map_err(|err| anyhow::anyhow!(
            "не удалось очистить незавершённую установку {}: {err}",
            install_dir.display()
        ))?;
}
```

- [ ] **Step 4: Run cleanup and installer checks**

Run:

```powershell
cargo test -p evohime-installer --test setup_cleanup_windows -- --nocapture
cargo test -p evohime-installer --test ui
cargo check -p evohime-installer
```

Expected: both cleanup tests and all UI tests pass; installer check exits with code `0`.

- [ ] **Step 5: Commit Task 2**

```powershell
git add -- crates/installer/src/setup_marker.rs crates/installer/src/lib.rs crates/installer/src/main.rs crates/installer/tests/setup_cleanup_windows.rs
git commit -m "fix(installer): require dirty setup cleanup"
```

---

### Task 3: Переключить весь установщик на свежую загрузку

**Files:**
- Modify: `crates/installer/src/main.rs:8-11,289-334`

**Interfaces:**
- Consumes: `download_fresh_and_verify(client, url, sha256_url, dest) -> Result<bool, DownloadError>` from Task 1
- Preserves: existing progress messages and checksum-mismatch messages
- Covers: five version archives plus `postgres.zip`

- [ ] **Step 1: Change the artifacts import**

Replace:

```rust
use evohime_artifacts::{download_with_resume_and_verify, extract_zip};
```

with:

```rust
use evohime_artifacts::{download_fresh_and_verify, extract_zip};
```

- [ ] **Step 2: Replace the five-asset loop**

Keep the existing `releases/latest/download` URL construction and progress text. Replace checksum fetching plus resume download with:

```rust
let ok = download_fresh_and_verify(&client, &url, &sha_url, &dest).await?;
if !ok {
    anyhow::bail!("SHA256 не совпадает для {asset_name} — прерываю установку");
}
```

The resulting loop must still enumerate exactly:

```rust
[
    "server.zip",
    "launcher.zip",
    "dist.zip",
    "migrations.zip",
    "worker.zip",
]
```

- [ ] **Step 3: Replace the PostgreSQL download**

Use the same fresh operation for `postgres.zip`:

```rust
let ok = download_fresh_and_verify(&client, &url, &sha_url, &pg_zip_path).await?;
if !ok {
    anyhow::bail!("SHA256 не совпадает для postgres.zip — прерываю установку");
}
```

Do not alter PostgreSQL extraction, ACL, initialization, or migration logic.

- [ ] **Step 4: Verify installer wiring and launcher non-regression**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p evohime-artifacts
cargo test -p evohime-launcher
cargo test -p evohime-installer --test setup_cleanup_windows
cargo test -p evohime-installer --test ui
cargo check -p evohime-installer
```

Expected: every command exits with code `0`; all test outputs report zero failures. The launcher suite proves its existing Range-resume path still works after adding the separate fresh-download API.

- [ ] **Step 5: Commit Task 3**

```powershell
git add -- crates/installer/src/main.rs
git commit -m "fix(installer): always fetch fresh release files"
```

---

### Task 4: Финальная проверка и очистка артефактов

**Files:**
- Verify: `crates/artifacts/src/downloader.rs`
- Verify: `crates/artifacts/src/lib.rs`
- Verify: `crates/installer/src/setup_marker.rs`
- Verify: `crates/installer/src/lib.rs`
- Verify: `crates/installer/src/main.rs`
- Verify: `crates/installer/tests/setup_cleanup_windows.rs`

**Interfaces:**
- Consumes: completed Tasks 1–3
- Produces: verified clean `main` with no workspace `target/`

- [ ] **Step 1: Inspect scope and formatting**

Run:

```powershell
git status --short
git diff --check HEAD~3..HEAD
cargo fmt --all -- --check
```

Expected: only task-related commits/files are present, diff check is empty, formatting exits with code `0`.

- [ ] **Step 2: Run all required verification commands**

Run:

```powershell
cargo test -p evohime-artifacts
cargo test -p evohime-launcher
cargo test -p evohime-installer --test setup_cleanup_windows
cargo test -p evohime-installer --test ui
cargo check -p evohime-installer
```

Expected: every command exits with code `0` and reports zero test failures.

- [ ] **Step 3: Remove build artifacts**

Run:

```powershell
cargo clean
Test-Path -LiteralPath 'C:\github\EvoHime\target'
```

Expected: `cargo clean` succeeds and `Test-Path` prints `False`.

- [ ] **Step 4: Confirm repository state**

Run:

```powershell
git status --short
git log -5 --oneline
```

Expected: no uncommitted task changes; the design/plan and three implementation commits are visible. Do not push unless the user explicitly requests it.
