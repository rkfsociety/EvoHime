use crate::{app::AppState, ApiError};
use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::process::Command;
use tokio::sync::Mutex;

pub const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/obra/superpowers-marketplace/main/.claude-plugin/marketplace.json";

pub const DEFAULT_CATALOG_SOURCES: &[&str] = &[
    DEFAULT_CATALOG_URL,
    "https://raw.githubusercontent.com/anthropics/claude-plugins-official/main/.claude-plugin/marketplace.json",
    "https://raw.githubusercontent.com/mhattingpete/claude-skills-marketplace/main/.claude-plugin/marketplace.json",
    "https://raw.githubusercontent.com/alirezarezvani/claude-skills/main/.claude-plugin/marketplace.json",
    "https://raw.githubusercontent.com/jeremylongshore/claude-code-plugins/main/.claude-plugin/marketplace.json",
];

const CATALOG_CACHE_TTL: Duration = Duration::from_secs(300);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub path: String,
    pub skills_count: usize,
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CatalogPlugin {
    pub name: String,
    pub description: String,
    pub version: String,
    pub category: String,
    pub group: String,
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    pub installable: bool,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CatalogGroup {
    pub id: String,
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginCatalogResponse {
    pub marketplace: String,
    pub source: String,
    pub sources: Vec<String>,
    pub groups: Vec<CatalogGroup>,
    pub plugins: Vec<CatalogPlugin>,
}

#[derive(Debug, Deserialize)]
struct PluginCatalogSettings {
    #[serde(default)]
    sources: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MarketplaceFile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    plugins: Vec<MarketplacePluginEntry>,
}

#[derive(Debug, Deserialize)]
struct MarketplacePluginEntry {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    source: MarketplaceSource,
}

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum MarketplaceSource {
    Object(MarketplaceSourceObject),
    Path(String),
    #[default]
    Missing,
}

#[derive(Debug, Deserialize)]
struct MarketplaceSourceObject {
    #[serde(default, rename = "source")]
    kind: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedCatalogEntry {
    name: String,
    description: String,
    version: String,
    category: String,
    group: String,
    source_url: Option<String>,
    source_path: Option<String>,
    git_ref: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedCatalog {
    fetched_at: DateTime<Utc>,
    source: String,
    sources: Vec<String>,
    marketplace: String,
    entries: Vec<ResolvedCatalogEntry>,
}

#[derive(Clone, Default)]
pub struct PluginCatalogCache {
    inner: Arc<Mutex<Option<CachedCatalog>>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginSkillSummary {
    pub name: String,
    pub preview: String,
}

#[derive(Debug, Deserialize)]
pub struct InstallPluginRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct PluginManifestFile {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
    #[serde(default)]
    skills: String,
    #[serde(default)]
    interface: Option<PluginInterface>,
}

#[derive(Debug, Deserialize)]
struct PluginInterface {
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
    #[serde(default, rename = "shortDescription")]
    short_description: Option<String>,
}

pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InstalledPlugin>>, ApiError> {
    let workspace_root = state.workspace_root.clone();
    let plugins = tokio::task::spawn_blocking(move || discover_installed_plugins(&workspace_root))
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(plugins))
}

pub async fn list_plugin_catalog(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PluginCatalogResponse>, ApiError> {
    let cached = load_catalog(&state).await?;
    let workspace_root = state.workspace_root.clone();
    let installed =
        tokio::task::spawn_blocking(move || discover_installed_plugins(&workspace_root))
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    let mut installed_lookup: HashMap<String, InstalledPlugin> = HashMap::new();
    for plugin in installed {
        installed_lookup.insert(plugin.name.clone(), plugin.clone());
        installed_lookup.insert(plugin.id.clone(), plugin);
    }

    let plugins = cached
        .entries
        .iter()
        .map(|entry| {
            let installed = installed_lookup
                .get(&entry.name)
                .or_else(|| installed_lookup.get(&entry.name.replace('_', "-")));
            CatalogPlugin {
                name: entry.name.clone(),
                description: entry.description.clone(),
                version: entry.version.clone(),
                category: entry.category.clone(),
                group: entry.group.clone(),
                source_url: entry.source_url.clone(),
                source_path: entry.source_path.clone(),
                r#ref: entry.git_ref.clone(),
                installable: entry.source_url.is_some(),
                installed: installed.is_some(),
                installed_version: installed.map(|plugin| plugin.version.clone()),
            }
        })
        .collect::<Vec<_>>();

    let groups = build_catalog_groups(&plugins);

    Ok(Json(PluginCatalogResponse {
        marketplace: cached.marketplace,
        source: cached.source,
        sources: cached.sources,
        groups,
        plugins,
    }))
}

pub async fn install_plugin(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InstallPluginRequest>,
) -> Result<Json<InstalledPlugin>, ApiError> {
    let installed = install_plugin_internal(&state, &body.name, false).await?;
    Ok(Json(installed))
}

pub async fn update_plugin(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InstallPluginRequest>,
) -> Result<Json<InstalledPlugin>, ApiError> {
    let installed = install_plugin_internal(&state, &body.name, true).await?;
    Ok(Json(installed))
}

pub async fn uninstall_plugin(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InstallPluginRequest>,
) -> Result<StatusCode, ApiError> {
    let name = sanitize_plugin_name(&body.name)
        .ok_or_else(|| ApiError::BadRequest("некорректное имя плагина".into()))?;
    let target = plugin_install_path(&state.workspace_root, &name);
    if !target.is_dir() {
        return Err(ApiError::BadRequest(format!(
            "плагин `{name}` не установлен"
        )));
    }
    tokio::fs::remove_dir_all(&target)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_plugin_skills(
    State(state): State<Arc<AppState>>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<Vec<PluginSkillSummary>>, ApiError> {
    let name = sanitize_plugin_name(&name)
        .ok_or_else(|| ApiError::BadRequest("некорректное имя плагина".into()))?;
    let workspace_root = state.workspace_root.clone();
    let skills = tokio::task::spawn_blocking(move || list_plugin_skill_summaries(&workspace_root, &name))
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .map_err(|message| ApiError::BadRequest(message))?;
    Ok(Json(skills))
}

async fn install_plugin_internal(
    state: &AppState,
    raw_name: &str,
    replace_existing: bool,
) -> Result<InstalledPlugin, ApiError> {
    let name = sanitize_plugin_name(raw_name)
        .ok_or_else(|| ApiError::BadRequest("некорректное имя плагина".into()))?;
    let cached = load_catalog(state).await?;
    let entry = cached
        .entries
        .iter()
        .find(|entry| entry.name == name)
        .cloned()
        .ok_or_else(|| ApiError::BadRequest(format!("плагин `{name}` не найден в каталоге")))?;
    let source_url = entry.source_url.clone().ok_or_else(|| {
        ApiError::BadRequest(format!("плагин `{name}` нельзя установить из каталога"))
    })?;
    validate_https_git_url(&source_url)?;
    if let Some(path) = entry.source_path.as_deref() {
        validate_source_subdir(path)?;
    }

    let target = plugin_install_path(&state.workspace_root, &name);
    if target.exists() {
        if !replace_existing {
            return Err(ApiError::Conflict(format!(
                "плагин `{name}` уже установлен в {}",
                target.display()
            )));
        }
        tokio::fs::remove_dir_all(&target)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    }

    let parent = target
        .parent()
        .ok_or_else(|| ApiError::Internal("некорректный путь установки плагина".into()))?
        .to_path_buf();
    tokio::fs::create_dir_all(&parent)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let clone_result = clone_plugin_repo(
        &source_url,
        entry.git_ref.as_deref(),
        entry.source_path.as_deref(),
        &target,
    )
    .await;
    if let Err(error) = clone_result {
        let _ = tokio::fs::remove_dir_all(&target).await;
        return Err(error);
    }

    let workspace_root = state.workspace_root.clone();
    tokio::task::spawn_blocking(move || load_installed_plugin(&workspace_root, &target))
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| {
            ApiError::Internal(
                "плагин склонирован, но manifest (.codex-plugin/.cursor-plugin/.claude-plugin) не найден"
                    .into(),
            )
        })
}

async fn load_catalog(state: &AppState) -> Result<CachedCatalog, ApiError> {
    {
        let guard = state.plugin_catalog_cache.inner.lock().await;
        if let Some(cached) = guard.as_ref() {
            let age = Utc::now().signed_duration_since(cached.fetched_at);
            if age.to_std().unwrap_or(Duration::MAX) < CATALOG_CACHE_TTL {
                return Ok(cached.clone());
            }
        }
    }

    let sources = resolve_catalog_sources(state).await?;
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let fetches = sources.iter().cloned().map(|source| {
        let client = client.clone();
        async move {
            let response = client.get(&source).send().await.map_err(|error| {
                (
                    source.clone(),
                    format!("каталог недоступен ({source}): {error}"),
                )
            })?;
            if !response.status().is_success() {
                return Err((
                    source.clone(),
                    format!("каталог {source} вернул HTTP {}", response.status()),
                ));
            }
            let body = response
                .text()
                .await
                .map_err(|error| (source.clone(), error.to_string()))?;
            Ok((source, body))
        }
    });

    let results = join_all(fetches).await;
    let mut marketplace_names = Vec::new();
    let mut used_sources = Vec::new();
    let mut entries = Vec::new();
    let mut seen_names = HashSet::new();
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok((source, body)) => {
                let marketplace_git = github_repo_from_raw_url(&source);
                match parse_marketplace_json(&body, marketplace_git.as_deref()) {
                    Ok(parsed) => {
                        marketplace_names.push(parsed.marketplace);
                        used_sources.push(source);
                        for entry in parsed.entries {
                            if seen_names.insert(entry.name.clone()) {
                                entries.push(entry);
                            }
                        }
                    }
                    Err(error) => errors.push(format!("{source}: {error}")),
                }
            }
            Err((_, message)) => errors.push(message),
        }
    }

    if entries.is_empty() {
        return Err(ApiError::Unavailable(format!(
            "не удалось загрузить каталоги: {}",
            errors.join("; ")
        )));
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));
    let marketplace = if marketplace_names.is_empty() {
        "evohime-catalog".to_string()
    } else {
        format!("evohime ({})", marketplace_names.join(" + "))
    };
    let cached = CachedCatalog {
        fetched_at: Utc::now(),
        source: used_sources
            .first()
            .cloned()
            .unwrap_or_else(|| DEFAULT_CATALOG_URL.to_string()),
        sources: used_sources,
        marketplace,
        entries,
    };
    *state.plugin_catalog_cache.inner.lock().await = Some(cached.clone());
    Ok(cached)
}

async fn resolve_catalog_sources(state: &AppState) -> Result<Vec<String>, ApiError> {
    if let Ok(url) = env::var("EVOHIME_PLUGIN_CATALOG_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return Ok(vec![trimmed.to_string()]);
        }
    }

    if let Some(value) = evohime_storage::load_setting(&state.pool, "plugin_catalog")
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        if let Ok(settings) = serde_json::from_value::<PluginCatalogSettings>(value) {
            let sources = settings
                .sources
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if !sources.is_empty() {
                return Ok(sources);
            }
        }
    }

    Ok(DEFAULT_CATALOG_SOURCES
        .iter()
        .map(|source| (*source).to_string())
        .collect())
}

#[derive(Debug)]
struct ParsedMarketplace {
    marketplace: String,
    entries: Vec<ResolvedCatalogEntry>,
}

fn parse_marketplace_json(
    raw: &str,
    marketplace_git_url: Option<&str>,
) -> Result<ParsedMarketplace, String> {
    let file: MarketplaceFile = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    let marketplace = if file.name.trim().is_empty() {
        "marketplace".to_string()
    } else {
        file.name
    };
    let mut entries = Vec::new();
    for plugin in file.plugins {
        let name = plugin.name.trim();
        if name.is_empty() {
            continue;
        }
        let (source_url, source_path, git_ref) =
            resolve_source(&plugin.source, marketplace_git_url);
        let category = normalize_category_label(
            &plugin.category,
            &plugin.name,
            &plugin.description,
            &plugin.keywords,
        );
        let group = classify_plugin_group(
            &category,
            &plugin.name,
            &plugin.description,
            &plugin.keywords,
        );
        entries.push(ResolvedCatalogEntry {
            name: name.to_string(),
            description: plugin.description,
            version: plugin.version,
            category,
            group,
            source_url,
            source_path,
            git_ref,
        });
    }
    Ok(ParsedMarketplace {
        marketplace,
        entries,
    })
}

fn resolve_source(
    source: &MarketplaceSource,
    marketplace_git_url: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    match source {
        MarketplaceSource::Missing => (None, None, None),
        MarketplaceSource::Path(path) => {
            let relative = normalize_relative_subdir(path);
            match (marketplace_git_url, relative) {
                (Some(repo), Some(subdir)) => (Some(repo.to_string()), Some(subdir), None),
                _ => (None, None, None),
            }
        }
        MarketplaceSource::Object(object) => {
            let kind = object.kind.trim().to_ascii_lowercase();
            if kind == "url" || kind.is_empty() {
                let url = object
                    .url
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let path = object.path.as_deref().and_then(normalize_relative_subdir);
                return (url, path, object.git_ref.clone());
            }
            if kind == "github" {
                if let Some(repo) = object
                    .repo
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    let url = format!("https://github.com/{repo}.git");
                    let path = object.path.as_deref().and_then(normalize_relative_subdir);
                    return (Some(url), path, object.git_ref.clone());
                }
            }
            if kind == "git-subdir" {
                let url = object
                    .url
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let path = object.path.as_deref().and_then(normalize_relative_subdir);
                return (url, path, object.git_ref.clone());
            }
            (None, None, None)
        }
    }
}

fn build_catalog_groups(plugins: &[CatalogPlugin]) -> Vec<CatalogGroup> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for plugin in plugins {
        *counts.entry(plugin.group.as_str()).or_default() += 1;
    }
    let mut groups = GROUP_ORDER
        .iter()
        .filter_map(|(id, label)| {
            let count = *counts.get(*id)?;
            (count > 0).then_some(CatalogGroup {
                id: (*id).to_string(),
                label: (*label).to_string(),
                count,
            })
        })
        .collect::<Vec<_>>();
    for (id, count) in counts {
        if GROUP_ORDER.iter().any(|(known, _)| *known == id) {
            continue;
        }
        groups.push(CatalogGroup {
            id: id.to_string(),
            label: id.to_string(),
            count,
        });
    }
    groups
}

const GROUP_ORDER: &[(&str, &str)] = &[
    ("development", "Разработка"),
    ("quality", "Тесты и качество"),
    ("security", "Безопасность"),
    ("infrastructure", "Инфра и DevOps"),
    ("ai", "AI / ML"),
    ("productivity", "Продуктивность"),
    ("business", "Бизнес и маркетинг"),
    ("product", "Продукт и процессы"),
    ("design", "Дизайн"),
    ("workflow", "Агентские workflow"),
    ("other", "Прочее"),
];

fn normalize_category_label(
    raw: &str,
    name: &str,
    description: &str,
    keywords: &[String],
) -> String {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        return trimmed.to_ascii_lowercase().replace('_', "-");
    }
    classify_plugin_group("", name, description, keywords)
}

fn classify_plugin_group(
    category: &str,
    name: &str,
    description: &str,
    keywords: &[String],
) -> String {
    let category = category.trim().to_ascii_lowercase().replace('_', "-");
    match category.as_str() {
        "development" | "engineering" | "api-development" | "database" => {
            return "development".into()
        }
        "testing" | "performance" | "quality" => return "quality".into(),
        "security" | "crypto" | "compliance" | "compliance-os" => return "security".into(),
        "devops" | "mcp" | "infrastructure" | "operations" => return "infrastructure".into(),
        "ai-ml" | "ai-agency" | "skill-enhancers" | "ai" => return "ai".into(),
        "productivity" | "documentation" | "community" => return "productivity".into(),
        "marketing" | "business-tools" | "business-growth" | "commercial" | "finance"
        | "saas-packs" => return "business".into(),
        "product" | "project-management" | "leadership" | "research" | "research-ops" => {
            return "product".into()
        }
        "design" => return "design".into(),
        "workflow" => return "workflow".into(),
        "other" | "" => {}
        _ => {}
    }

    let haystack = format!(
        "{} {} {}",
        name.to_ascii_lowercase(),
        description.to_ascii_lowercase(),
        keywords
            .iter()
            .map(|item| item.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    );

    let rules: &[(&str, &[&str])] = &[
        (
            "workflow",
            &[
                "superpowers",
                "workflow",
                "brainstorm",
                "tdd",
                "debugging",
                "code review",
                "session-driver",
                "plan",
            ],
        ),
        (
            "security",
            &[
                "security",
                "owasp",
                "auth",
                "vulnerability",
                "compliance",
                "crypto",
            ],
        ),
        (
            "infrastructure",
            &[
                "devops",
                "docker",
                "kubernetes",
                "ci/cd",
                "terraform",
                "mcp",
                "deploy",
            ],
        ),
        (
            "ai",
            &[
                "llm",
                "embedding",
                "model",
                "agent sdk",
                "prompt",
                "rag",
                "ml ",
            ],
        ),
        (
            "quality",
            &["test", "qa", "lint", "performance", "benchmark", "coverage"],
        ),
        (
            "business",
            &[
                "marketing",
                "seo",
                "sales",
                "finance",
                "crm",
                "growth",
                "saas",
            ],
        ),
        (
            "product",
            &[
                "product",
                "roadmap",
                "research",
                "leadership",
                "pm ",
                "project management",
            ],
        ),
        (
            "design",
            &["design", "ui", "ux", "figma", "visual", "creative"],
        ),
        (
            "productivity",
            &[
                "productivity",
                "documentation",
                "notes",
                "journal",
                "writing",
            ],
        ),
        (
            "development",
            &[
                "api", "sdk", "backend", "frontend", "database", "code", "refactor",
            ],
        ),
    ];

    for (group, needles) in rules {
        if needles.iter().any(|needle| haystack.contains(needle)) {
            return (*group).to_string();
        }
    }
    "other".to_string()
}

fn github_repo_from_raw_url(url: &str) -> Option<String> {
    let prefix = "https://raw.githubusercontent.com/";
    let rest = url.strip_prefix(prefix)?;
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("https://github.com/{owner}/{repo}.git"))
}

fn normalize_relative_subdir(path: &str) -> Option<String> {
    let trimmed = path.trim().trim_start_matches("./");
    if trimmed.is_empty() {
        return None;
    }
    validate_source_subdir(trimmed).ok()?;
    Some(trimmed.replace('\\', "/"))
}

fn validate_source_subdir(path: &str) -> Result<(), ApiError> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(ApiError::BadRequest(
            "subdir плагина не должен быть абсолютным".into(),
        ));
    }
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err(ApiError::BadRequest("некорректный subdir плагина".into())),
        }
    }
    Ok(())
}

fn validate_https_git_url(url: &str) -> Result<(), ApiError> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(ApiError::BadRequest(
            "разрешены только http(s) git URL для установки плагинов".into(),
        ));
    }
    if trimmed.contains(' ') || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(ApiError::BadRequest("некорректный git URL".into()));
    }
    Ok(())
}

fn plugin_install_path(workspace_root: &Path, name: &str) -> PathBuf {
    workspace_root.join(".evohime").join("plugins").join(name)
}

fn list_plugin_skill_summaries(
    workspace_root: &Path,
    plugin_id: &str,
) -> Result<Vec<PluginSkillSummary>, String> {
    let target = plugin_install_path(workspace_root, plugin_id);
    if !target.is_dir() {
        return Err(format!("плагин `{plugin_id}` не установлен"));
    }
    let manifest = read_plugin_manifest(&target)
        .ok_or_else(|| "manifest плагина не найден".to_string())?;
    let skills_root = resolve_skills_root(&target, &manifest.skills)
        .ok_or_else(|| "каталог skills не найден".to_string())?;
    Ok(list_skill_summaries(&skills_root))
}

fn list_skill_summaries(skills_root: &Path) -> Vec<PluginSkillSummary> {
    list_skill_names(skills_root)
        .into_iter()
        .map(|name| {
            let preview = read_skill_preview(&skills_root.join(&name));
            PluginSkillSummary { name, preview }
        })
        .collect()
}

fn read_skill_preview(skill_dir: &Path) -> String {
    let path = skill_dir.join("SKILL.md");
    let Ok(content) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let normalized = content.trim();
    if normalized.chars().count() <= 400 {
        return normalized.to_string();
    }
    normalized.chars().take(400).collect::<String>() + "…"
}

fn sanitize_plugin_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        return None;
    }
    if trimmed.contains("..") || trimmed.starts_with('.') {
        return None;
    }
    Some(trimmed.to_string())
}

async fn clone_plugin_repo(
    source_url: &str,
    git_ref: Option<&str>,
    source_path: Option<&str>,
    target: &Path,
) -> Result<(), ApiError> {
    let parent = target
        .parent()
        .ok_or_else(|| ApiError::Internal("некорректный путь установки".into()))?;
    let clone_target = if source_path.is_some() {
        parent.join(format!(
            ".tmp-{}",
            target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("plugin")
        ))
    } else {
        target.to_path_buf()
    };

    if clone_target.exists() {
        tokio::fs::remove_dir_all(&clone_target)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    }

    let target_for_git = path_for_external_cli(&clone_target);
    let mut command = Command::new("git");
    command.arg("clone").arg("--depth").arg("1");
    if let Some(git_ref) = git_ref.filter(|value| !value.trim().is_empty()) {
        command.arg("--branch").arg(git_ref);
    }
    command.arg(source_url).arg(&target_for_git);
    let output = tokio::time::timeout(INSTALL_TIMEOUT, command.output())
        .await
        .map_err(|_| ApiError::Internal("таймаут git clone при установке плагина".into()))?
        .map_err(|error| ApiError::Internal(format!("не удалось запустить git: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = tokio::fs::remove_dir_all(&clone_target).await;
        return Err(ApiError::Internal(format!(
            "git clone завершился с ошибкой: {}",
            stderr.trim()
        )));
    }

    if let Some(subdir) = source_path {
        let source_dir = clone_target.join(subdir);
        if !source_dir.is_dir() {
            let _ = tokio::fs::remove_dir_all(&clone_target).await;
            return Err(ApiError::Internal(format!(
                "в репозитории нет каталога плагина `{subdir}`"
            )));
        }
        tokio::fs::rename(&source_dir, target)
            .await
            .map_err(|error| ApiError::Internal(format!("не удалось перенести плагин: {error}")))?;
        let _ = tokio::fs::remove_dir_all(&clone_target).await;
    }

    Ok(())
}

/// Git and many Windows CLIs reject `\\?\` verbatim paths from `canonicalize()`.
fn path_for_external_cli(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{stripped}"));
    }
    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }
    path.to_path_buf()
}

fn discover_installed_plugins(workspace_root: &Path) -> Vec<InstalledPlugin> {
    let root = workspace_root.join(".evohime").join("plugins");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut plugins = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = path.file_name()?.to_string_lossy();
            if name.starts_with('.') {
                return None;
            }
            load_installed_plugin(workspace_root, &path)
        })
        .collect::<Vec<_>>();
    plugins.sort_by(|left, right| left.name.cmp(&right.name));
    plugins
}

fn load_installed_plugin(workspace_root: &Path, plugin_root: &Path) -> Option<InstalledPlugin> {
    let id = plugin_root.file_name()?.to_str()?.to_string();
    let manifest = read_plugin_manifest(plugin_root)?;
    let skills = resolve_skills_root(plugin_root, &manifest.skills)
        .map(|root| list_skill_names(&root))
        .unwrap_or_default();
    let display_name = manifest
        .interface
        .as_ref()
        .and_then(|value| value.display_name.clone())
        .or(manifest.display_name)
        .unwrap_or_else(|| manifest.name.clone());
    let description = manifest
        .interface
        .as_ref()
        .and_then(|value| value.short_description.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(manifest.description);
    let relative = plugin_root
        .strip_prefix(workspace_root)
        .unwrap_or(plugin_root)
        .to_string_lossy()
        .replace('\\', "/");

    Some(InstalledPlugin {
        id,
        name: manifest.name,
        display_name,
        version: manifest.version,
        description,
        path: relative,
        skills_count: skills.len(),
        skills,
    })
}

fn read_plugin_manifest(plugin_root: &Path) -> Option<PluginManifestFile> {
    for relative in [
        ".codex-plugin/plugin.json",
        ".cursor-plugin/plugin.json",
        ".claude-plugin/plugin.json",
    ] {
        let path = plugin_root.join(relative);
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(manifest) = serde_json::from_str::<PluginManifestFile>(&contents) {
                return Some(manifest);
            }
        }
    }
    None
}

fn resolve_skills_root(plugin_root: &Path, skills: &str) -> Option<PathBuf> {
    let trimmed = skills.trim();
    let relative = if trimmed.is_empty() {
        "skills"
    } else {
        trimmed.trim_start_matches("./")
    };
    let skills_root = plugin_root.join(relative);
    if skills_root.starts_with(plugin_root) && skills_root.is_dir() {
        Some(skills_root)
    } else {
        None
    }
}

fn list_skill_names(skills_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(skills_root) else {
        return Vec::new();
    };
    let mut skills = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("SKILL.md").is_file())
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    skills.sort();
    skills
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_codex_plugin_with_skills() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plugin = temp.path().join(".evohime/plugins/demo");
        fs::create_dir_all(plugin.join(".codex-plugin")).expect("meta");
        fs::create_dir_all(plugin.join("skills/bootstrap")).expect("skill");
        fs::write(
            plugin.join(".codex-plugin/plugin.json"),
            r#"{
              "name": "demo",
              "version": "1.2.3",
              "description": "Demo plugin",
              "skills": "./skills/",
              "interface": {
                "displayName": "Demo Pack",
                "shortDescription": "Short demo"
              }
            }"#,
        )
        .expect("manifest");
        fs::write(plugin.join("skills/bootstrap/SKILL.md"), "# Bootstrap\n").expect("skill md");

        let plugins = discover_installed_plugins(temp.path());
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "demo");
        assert_eq!(plugins[0].display_name, "Demo Pack");
        assert_eq!(plugins[0].description, "Short demo");
        assert_eq!(plugins[0].version, "1.2.3");
        assert_eq!(plugins[0].path, ".evohime/plugins/demo");
        assert_eq!(plugins[0].skills, vec!["bootstrap".to_string()]);
        assert_eq!(plugins[0].skills_count, 1);
    }

    #[test]
    fn lists_skill_previews_for_installed_plugin() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plugin = temp.path().join(".evohime/plugins/demo");
        fs::create_dir_all(plugin.join(".codex-plugin")).expect("meta");
        fs::create_dir_all(plugin.join("skills/bootstrap")).expect("skill");
        fs::write(
            plugin.join(".codex-plugin/plugin.json"),
            r#"{"name":"demo","version":"1.0.0","description":"Demo","skills":"./skills/"}"#,
        )
        .expect("manifest");
        fs::write(
            plugin.join("skills/bootstrap/SKILL.md"),
            "# Bootstrap skill\n\nUse this before planning.",
        )
        .expect("skill md");

        let skills = list_plugin_skill_summaries(temp.path(), "demo").expect("skills");
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "bootstrap");
        assert!(skills[0].preview.contains("Bootstrap skill"));
    }

    #[test]
    fn returns_empty_when_plugins_dir_missing() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(discover_installed_plugins(temp.path()).is_empty());
    }

    #[test]
    fn parses_remote_marketplace_fixture_with_relative_and_subdir() {
        let raw = r#"{
          "name": "superpowers-marketplace",
          "plugins": [
            {
              "name": "superpowers",
              "description": "Core skills",
              "version": "6.1.1",
              "source": { "source": "url", "url": "https://github.com/obra/superpowers.git" }
            },
            {
              "name": "local-only",
              "description": "relative",
              "version": "1.0.0",
              "source": "./plugins/local"
            },
            {
              "name": "github-plugin",
              "description": "via github",
              "version": "2.0.0",
              "source": { "source": "github", "repo": "obra/example", "ref": "dev" }
            },
            {
              "name": "subdir-plugin",
              "description": "git-subdir",
              "version": "1.0.0",
              "source": {
                "source": "git-subdir",
                "url": "https://github.com/acme/bundle.git",
                "path": "plugins/demo",
                "ref": "main"
              }
            }
          ]
        }"#;
        let marketplace_git = "https://github.com/obra/superpowers-marketplace.git";
        let parsed = parse_marketplace_json(raw, Some(marketplace_git)).expect("parse");
        assert_eq!(parsed.marketplace, "superpowers-marketplace");
        assert_eq!(parsed.entries.len(), 4);
        assert_eq!(
            parsed.entries[0].source_url.as_deref(),
            Some("https://github.com/obra/superpowers.git")
        );
        assert_eq!(
            parsed.entries[1].source_url.as_deref(),
            Some(marketplace_git)
        );
        assert_eq!(
            parsed.entries[1].source_path.as_deref(),
            Some("plugins/local")
        );
        assert_eq!(
            parsed.entries[2].source_url.as_deref(),
            Some("https://github.com/obra/example.git")
        );
        assert_eq!(parsed.entries[2].git_ref.as_deref(), Some("dev"));
        assert_eq!(
            parsed.entries[3].source_path.as_deref(),
            Some("plugins/demo")
        );
        assert_eq!(parsed.entries[0].group, "workflow");
    }

    #[test]
    fn classifies_marketplace_categories_into_groups() {
        assert_eq!(
            classify_plugin_group("devops", "k8s-helper", "", &[]),
            "infrastructure"
        );
        assert_eq!(
            classify_plugin_group("saas-packs", "billing-pack", "", &[]),
            "business"
        );
        assert_eq!(
            classify_plugin_group("", "superpowers", "TDD debugging workflows", &[]),
            "workflow"
        );
        assert_eq!(
            classify_plugin_group("security", "api-audit", "", &[]),
            "security"
        );
    }

    #[test]
    fn derives_github_repo_from_raw_marketplace_url() {
        assert_eq!(
            github_repo_from_raw_url(DEFAULT_CATALOG_URL).as_deref(),
            Some("https://github.com/obra/superpowers-marketplace.git")
        );
    }

    #[test]
    fn matches_installed_plugin_by_catalog_name() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plugin = temp.path().join(".evohime/plugins/superpowers");
        fs::create_dir_all(plugin.join(".codex-plugin")).expect("meta");
        fs::create_dir_all(plugin.join("skills/using-superpowers")).expect("skill");
        fs::write(
            plugin.join(".codex-plugin/plugin.json"),
            r#"{"name":"superpowers","version":"6.1.1","skills":"./skills/"}"#,
        )
        .expect("manifest");
        fs::write(
            plugin.join("skills/using-superpowers/SKILL.md"),
            "# using\n",
        )
        .expect("skill");

        let installed = discover_installed_plugins(temp.path());
        let parsed = parse_marketplace_json(
            r#"{
              "name":"m",
              "plugins":[{"name":"superpowers","version":"6.1.1","source":{"source":"url","url":"https://github.com/obra/superpowers.git"}}]
            }"#,
            None,
        )
        .expect("parse");
        let match_name = &parsed.entries[0].name;
        assert!(installed.iter().any(|plugin| plugin.name == *match_name));
    }

    #[test]
    fn rejects_non_https_and_bad_names() {
        assert!(validate_https_git_url("https://github.com/obra/superpowers.git").is_ok());
        assert!(validate_https_git_url("git@github.com:obra/superpowers.git").is_err());
        assert!(sanitize_plugin_name("superpowers").is_some());
        assert!(sanitize_plugin_name("../evil").is_none());
        assert!(sanitize_plugin_name("a/b").is_none());
        assert!(validate_source_subdir("../evil").is_err());
        assert!(validate_source_subdir("plugins/demo").is_ok());
    }

    #[test]
    fn strips_windows_verbatim_prefix_for_git() {
        let verbatim = PathBuf::from(r"\\?\F:\github\EvoHime\.evohime\plugins\superpowers-chrome");
        assert_eq!(
            path_for_external_cli(&verbatim),
            PathBuf::from(r"F:\github\EvoHime\.evohime\plugins\superpowers-chrome")
        );
        let unc = PathBuf::from(r"\\?\UNC\server\share\plugins\demo");
        assert_eq!(
            path_for_external_cli(&unc),
            PathBuf::from(r"\\server\share\plugins\demo")
        );
        let normal = PathBuf::from(r"F:\github\EvoHime\.evohime\plugins\demo");
        assert_eq!(path_for_external_cli(&normal), normal);
    }

    #[test]
    fn settings_json_shape_is_sources_array() {
        let value: serde_json::Value = serde_json::json!({ "sources": [DEFAULT_CATALOG_URL] });
        let settings: PluginCatalogSettings = serde_json::from_value(value).expect("settings");
        assert_eq!(settings.sources.len(), 1);
        assert_eq!(DEFAULT_CATALOG_SOURCES.len(), 5);
    }
}
