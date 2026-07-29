//! Единственное место, где Launcher обращается к GitHub REST API (раздел V
//! плана) — только в момент реального клика "Обновить сейчас", для
//! получения точных ссылок на ассеты последнего релиза и их SHA256.
//! Фоновая проверка (`update_check.rs`) использует Atom-фид, чтобы не
//! попасть в лимит 60 запросов/час на IP.

use crate::update_apply::ReleaseAsset;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum GitHubApiError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("missing expected release asset: {0}")]
    MissingAsset(&'static str),
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    assets: Vec<ReleaseAssetResponse>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAssetResponse {
    name: String,
    browser_download_url: String,
}

const REQUIRED_ASSETS: &[&str] = &["server.zip", "dist.zip", "migrations.zip", "worker.zip"];

/// `evohime-launcher.exe` сам себя никогда не обновляет через основной
/// 4-ассетный цикл (тот атомарно переключает `versions/<version>/`, а
/// launcher живёт вне этой схемы, по фиксированному пути и с процессом,
/// который нужно сначала завершить). Поэтому этот ассет ищем отдельно,
/// best-effort — его отсутствие в релизе не должно блокировать основное
/// обновление server/dist/migrations/worker.
const LAUNCHER_ASSET: &str = "launcher.zip";

/// Возвращает тег последнего релиза, список найденных обязательных ассетов
/// (только те из `REQUIRED_ASSETS`, для которых реально нашёлся и сам
/// файл, и его `*.sha256` в релизе — отсутствие обязательного ассета не
/// является ошибкой сама по себе, чтобы можно было тестировать частичные
/// релизы; вызывающий код решает, что делать при неполном списке) и
/// отдельно — необязательный `launcher.zip`, если он есть в релизе.
pub async fn fetch_latest_release(
    client: &reqwest::Client,
    github_repo: &str,
) -> Result<(String, Vec<ReleaseAsset>, Option<ReleaseAsset>), GitHubApiError> {
    let url = format!("https://api.github.com/repos/{github_repo}/releases/latest");
    let response = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, "EvoHime-Launcher")
        .send()
        .await?
        .error_for_status()?
        .json::<ReleaseResponse>()
        .await?;

    let assets = collect_required_assets(&response.assets);
    let launcher_asset = find_asset(&response.assets, LAUNCHER_ASSET);
    Ok((response.tag_name, assets, launcher_asset))
}

fn collect_required_assets(assets: &[ReleaseAssetResponse]) -> Vec<ReleaseAsset> {
    let mut result = Vec::new();
    for &name in REQUIRED_ASSETS {
        if let Some(found) = find_asset(assets, name) {
            result.push(found);
        }
    }
    result
}

fn find_asset(assets: &[ReleaseAssetResponse], name: &str) -> Option<ReleaseAsset> {
    let file_asset = assets.iter().find(|a| a.name == name)?;
    let sha_name = format!("{name}.sha256");
    let sha_asset = assets.iter().find(|a| a.name == sha_name)?;
    Some(ReleaseAsset {
        file_name: name.to_string(),
        download_url: file_asset.browser_download_url.clone(),
        sha256_url: sha_asset.browser_download_url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, url: &str) -> ReleaseAssetResponse {
        ReleaseAssetResponse {
            name: name.to_string(),
            browser_download_url: url.to_string(),
        }
    }

    #[test]
    fn collects_only_assets_with_matching_sha256_present() {
        let assets = vec![
            asset("server.zip", "https://example.com/server.zip"),
            asset("server.zip.sha256", "https://example.com/server.zip.sha256"),
            asset("dist.zip", "https://example.com/dist.zip"),
            // dist.zip.sha256 deliberately missing
        ];

        let collected = collect_required_assets(&assets);
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].file_name, "server.zip");
    }

    #[test]
    fn collects_all_when_fully_present() {
        let mut assets = Vec::new();
        for name in REQUIRED_ASSETS {
            assets.push(asset(name, &format!("https://example.com/{name}")));
            assets.push(asset(
                &format!("{name}.sha256"),
                &format!("https://example.com/{name}.sha256"),
            ));
        }

        let collected = collect_required_assets(&assets);
        assert_eq!(collected.len(), REQUIRED_ASSETS.len());
    }

    #[test]
    fn returns_empty_when_nothing_matches() {
        let assets = vec![asset("readme.md", "https://example.com/readme.md")];
        assert!(collect_required_assets(&assets).is_empty());
    }

    #[test]
    fn finds_optional_launcher_asset_when_present() {
        let assets = vec![
            asset("launcher.zip", "https://example.com/launcher.zip"),
            asset(
                "launcher.zip.sha256",
                "https://example.com/launcher.zip.sha256",
            ),
        ];
        let found = find_asset(&assets, LAUNCHER_ASSET).expect("launcher.zip should be found");
        assert_eq!(found.file_name, "launcher.zip");
    }

    #[test]
    fn launcher_asset_absent_does_not_affect_required_assets() {
        let mut assets = Vec::new();
        for name in REQUIRED_ASSETS {
            assets.push(asset(name, &format!("https://example.com/{name}")));
            assets.push(asset(
                &format!("{name}.sha256"),
                &format!("https://example.com/{name}.sha256"),
            ));
        }
        // Нет launcher.zip в этом релизе.
        assert_eq!(
            collect_required_assets(&assets).len(),
            REQUIRED_ASSETS.len()
        );
        assert!(find_asset(&assets, LAUNCHER_ASSET).is_none());
    }
}
