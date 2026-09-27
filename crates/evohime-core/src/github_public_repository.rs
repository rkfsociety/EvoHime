use std::time::Duration;

#[cfg(not(test))]
use std::sync::OnceLock;

use futures_util::StreamExt;
use reqwest::{header, redirect::Policy, Client, Response, Url};
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use tokio::sync::Semaphore;

use crate::integration_provider_sdk::{
    canonical_hash, AuthMethod, IntegrationActionV1, IntegrationProviderManifestV1, RiskClass,
};

const API_BASE: &str = "https://api.github.com/";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_ITEMS: usize = 10;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(25);

#[cfg(not(test))]
static REFRESH_LIMIT: OnceLock<Semaphore> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RepositoryId {
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RepositoryItem {
    pub number: u64,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RepositoryProjection {
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars: u64,
    pub forks: u64,
    pub open_issues: u64,
    pub issues: Vec<RepositoryItem>,
    pub pull_requests: Vec<RepositoryItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchError {
    Busy,
    NotFound,
    RateLimited,
    TimedOut,
    Network,
    Oversized,
    InvalidResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RepositoryNameError {
    Owner,
    Repo,
}

impl RepositoryId {
    pub fn parse(owner: String, repo: String) -> Result<Self, RepositoryNameError> {
        if !valid_segment(&owner, 39, false) {
            return Err(RepositoryNameError::Owner);
        }
        if !valid_segment(&repo, 100, true) {
            return Err(RepositoryNameError::Repo);
        }
        Ok(Self { owner, repo })
    }
}

type ManifestResult =
    Result<IntegrationProviderManifestV1, crate::integration_provider_sdk::SdkError>;

pub(crate) fn manifest() -> ManifestResult {
    let mut manifest = IntegrationProviderManifestV1 {
        schema_version: 1,
        id: "github.public".into(),
        version: 1,
        display_name: "GitHub — публичные репозитории".into(),
        description: concat!(
            "Просмотр сводки и открытых issues/pull requests публичного репозитория. ",
            "Только чтение, без входа в аккаунт."
        )
        .into(),
        auth_methods: vec![AuthMethod::None],
        actions: vec![IntegrationActionV1 {
            id: "repository.read".into(),
            version: 1,
            description: concat!(
                "Загрузить сводку, открытые issues и pull requests сохранённого ",
                "публичного репозитория."
            )
            .into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{"owner":{"type":"string"},"repo":{"type":"string"}},
                "required":["owner","repo"]
            }),
            output_schema: serde_json::json!({"type":"object"}),
            required_scopes: Vec::new(),
            risk_class: RiskClass::ReadOnly,
            side_effect_class: "read_only_public_http".into(),
            idempotency_support: true,
        }],
        triggers: Vec::new(),
        credential_schema: serde_json::json!({"type":"object"}),
        content_hash: String::new(),
    };
    manifest.content_hash = canonical_hash(&manifest).map_err(|_| {
        crate::integration_provider_sdk::SdkError::Unavailable(
            "GitHub manifest serialization failed".into(),
        )
    })?;
    Ok(manifest)
}

fn valid_segment(value: &str, max_len: usize, allow_dot_underscore: bool) -> bool {
    if value.is_empty() || value.len() > max_len || value == "." || value == ".." {
        return false;
    }
    value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || byte == b'-'
            || (allow_dot_underscore && matches!(byte, b'.' | b'_'))
    })
}

#[derive(Deserialize)]
struct RepositoryResponse {
    full_name: String,
    description: Option<String>,
    language: Option<String>,
    stargazers_count: u64,
    forks_count: u64,
    open_issues_count: u64,
}

#[derive(Deserialize)]
struct IssueResponse {
    number: u64,
    title: String,
    pull_request: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct PullRequestResponse {
    number: u64,
    title: String,
}

pub(crate) struct GitHubPublicRepositoryClient {
    client: Client,
    api_base: Url,
}

impl GitHubPublicRepositoryClient {
    pub fn new() -> Result<Self, FetchError> {
        Self::with_base(API_BASE)
    }

    fn with_base(base: &str) -> Result<Self, FetchError> {
        let api_base = Url::parse(base).map_err(|_| FetchError::Network)?;
        if api_base.scheme() != "https"
            || api_base.host_str() != Some("api.github.com")
            || !api_base.username().is_empty()
            || api_base.password().is_some()
            || api_base.query().is_some()
            || api_base.fragment().is_some()
        {
            #[cfg(not(test))]
            return Err(FetchError::Network);
        }
        let builder = Client::builder()
            .redirect(Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .user_agent("EvoHime-Desktop")
            .default_headers({
                let mut headers = header::HeaderMap::new();
                headers.insert(
                    header::ACCEPT,
                    header::HeaderValue::from_static("application/vnd.github+json"),
                );
                headers.insert(
                    header::HeaderName::from_static("x-github-api-version"),
                    header::HeaderValue::from_static("2026-03-10"),
                );
                headers
            });
        #[cfg(not(test))]
        let builder = builder.https_only(true);
        let client = builder.build().map_err(|_| FetchError::Network)?;
        Ok(Self { client, api_base })
    }

    fn endpoint(
        &self,
        repository: &RepositoryId,
        resource: Option<&str>,
    ) -> Result<Url, FetchError> {
        let mut url = self.api_base.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| FetchError::Network)?;
            segments.pop_if_empty();
            segments.push("repos");
            segments.push(&repository.owner);
            segments.push(&repository.repo);
            if let Some(resource) = resource {
                segments.push(resource);
            }
        }
        if resource == Some("issues") {
            url.query_pairs_mut()
                .append_pair("state", "open")
                .append_pair("per_page", "10");
        } else if resource == Some("pulls") {
            url.query_pairs_mut()
                .append_pair("state", "open")
                .append_pair("per_page", "10");
        }
        Ok(url)
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T, FetchError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    FetchError::TimedOut
                } else {
                    FetchError::Network
                }
            })?;
        ensure_success(&response)?;
        let bytes = bounded_response(response).await?;
        serde_json::from_slice(&bytes).map_err(|_| FetchError::InvalidResponse)
    }

    pub async fn fetch(
        &self,
        repository: &RepositoryId,
    ) -> Result<RepositoryProjection, FetchError> {
        #[cfg(not(test))]
        let _permit = REFRESH_LIMIT
            .get_or_init(|| Semaphore::new(1))
            .try_acquire()
            .map_err(|_| FetchError::Busy)?;

        tokio::time::timeout(REFRESH_TIMEOUT, async {
            let metadata: RepositoryResponse =
                self.get_json(self.endpoint(repository, None)?).await?;
            let issues: Vec<IssueResponse> = self
                .get_json(self.endpoint(repository, Some("issues"))?)
                .await?;
            let pull_requests: Vec<PullRequestResponse> = self
                .get_json(self.endpoint(repository, Some("pulls"))?)
                .await?;

            let full_name = bounded_text(metadata.full_name, 140);
            let (owner, repo) = full_name
                .split_once('/')
                .filter(|(owner, repo)| {
                    !owner.is_empty()
                        && !repo.is_empty()
                        && owner.eq_ignore_ascii_case(&repository.owner)
                        && repo.eq_ignore_ascii_case(&repository.repo)
                })
                .ok_or(FetchError::InvalidResponse)?;
            let issues = issues
                .into_iter()
                .filter(|issue| issue.pull_request.is_none())
                .take(MAX_ITEMS)
                .map(|issue| RepositoryItem {
                    number: issue.number,
                    title: bounded_text(issue.title, 240),
                    url: format!("https://github.com/{owner}/{repo}/issues/{}", issue.number),
                })
                .collect();
            let pull_requests = pull_requests
                .into_iter()
                .take(MAX_ITEMS)
                .map(|pull| RepositoryItem {
                    number: pull.number,
                    title: bounded_text(pull.title, 240),
                    url: format!("https://github.com/{owner}/{repo}/pull/{}", pull.number),
                })
                .collect();

            Ok(RepositoryProjection {
                full_name,
                description: metadata.description.map(|value| bounded_text(value, 800)),
                language: metadata.language.map(|value| bounded_text(value, 80)),
                stars: metadata.stargazers_count,
                forks: metadata.forks_count,
                open_issues: metadata.open_issues_count,
                issues,
                pull_requests,
            })
        })
        .await
        .map_err(|_| FetchError::TimedOut)?
    }
}

fn ensure_success(response: &Response) -> Result<(), FetchError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(FetchError::NotFound);
    }
    if status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(FetchError::RateLimited);
    }
    Err(FetchError::Network)
}

async fn bounded_response(response: Response) -> Result<Vec<u8>, FetchError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(FetchError::Oversized);
    }
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| FetchError::Network)?;
        if output.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(FetchError::Oversized);
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn bounded_text(value: String, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn repository_identifiers_reject_urls_and_path_segments() {
        assert!(RepositoryId::parse("owner".into(), "repo".into()).is_ok());
        for bad in ["../repo", "owner/repo", "https://github.com/o/r", "repo?x=1"] {
            assert!(RepositoryId::parse("owner".into(), bad.into()).is_err());
        }
        assert!(RepositoryId::parse("owner/name".into(), "repo".into()).is_err());
    }

    #[test]
    fn provider_manifest_is_valid_and_read_only() {
        let manifest = manifest().expect("manifest");
        assert_eq!(manifest.id, "github.public");
        assert!(crate::integration_provider_sdk::validate_manifest(&manifest).is_ok());
        assert!(manifest.actions.iter().all(|action| {
            action.risk_class == crate::integration_provider_sdk::RiskClass::ReadOnly
        }));
    }

    #[test]
    fn endpoint_is_fixed_and_encodes_path_segments() {
        let client = GitHubPublicRepositoryClient::new().expect("fixed GitHub client");
        let repository = RepositoryId::parse("Some-Owner".into(), "a.repo".into())
            .expect("valid identifier");
        let url = client
            .endpoint(&repository, Some("issues"))
            .expect("endpoint");
        assert_eq!(url.host_str(), Some("api.github.com"));
        assert_eq!(url.path(), "/repos/Some-Owner/a.repo/issues");
        assert_eq!(url.query(), Some("state=open&per_page=10"));
    }

    #[tokio::test]
    async fn fetch_returns_bounded_separated_read_only_projection() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/Hello-World"))
            .and(header("accept", "application/vnd.github+json"))
            .and(header("x-github-api-version", "2026-03-10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "full_name":"octocat/Hello-World", "description":"Example", "language":"Rust",
                "stargazers_count":12, "forks_count":3, "open_issues_count":2
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/Hello-World/issues"))
            .and(query_param("state", "open"))
            .and(query_param("per_page", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"number":1,"title":"Issue","pull_request":null},
                {"number":2,"title":"PR listed as issue","pull_request":{"url":"ignored"}}
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/Hello-World/pulls"))
            .and(query_param("state", "open"))
            .and(query_param("per_page", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"number":2,"title":"Pull request"}
            ])))
            .mount(&server)
            .await;

        let client = GitHubPublicRepositoryClient::with_base(&format!("{}/", server.uri()))
            .expect("mock client");
        let repository = RepositoryId::parse("octocat".into(), "Hello-World".into())
            .expect("valid repository");
        let projection = client.fetch(&repository).await.expect("projection");
        assert_eq!(projection.full_name, "octocat/Hello-World");
        assert_eq!(projection.issues.len(), 1);
        assert_eq!(projection.issues[0].number, 1);
        assert_eq!(projection.pull_requests.len(), 1);
        assert_eq!(
            projection.pull_requests[0].url,
            "https://github.com/octocat/Hello-World/pull/2"
        );
    }

    #[tokio::test]
    async fn fetch_classifies_rate_limit_redirect_and_oversized_response() {
        let server = MockServer::start().await;
        let repository = RepositoryId::parse("owner".into(), "repo".into()).expect("repository");
        let client = GitHubPublicRepositoryClient::with_base(&format!("{}/", server.uri()))
            .expect("mock client");
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        assert_eq!(client.fetch(&repository).await, Err(FetchError::RateLimited));

        let second_server = MockServer::start().await;
        let redirect_client =
            GitHubPublicRepositoryClient::with_base(&format!("{}/", second_server.uri()))
                .expect("mock client");
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", "https://example.invalid/"),
            )
            .mount(&second_server)
            .await;
        assert_eq!(redirect_client.fetch(&repository).await, Err(FetchError::Network));

        let third_server = MockServer::start().await;
        let oversized_client =
            GitHubPublicRepositoryClient::with_base(&format!("{}/", third_server.uri()))
                .expect("mock client");
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("x".repeat(MAX_RESPONSE_BYTES + 1)),
            )
            .mount(&third_server)
            .await;
        assert_eq!(
            oversized_client.fetch(&repository).await,
            Err(FetchError::Oversized)
        );

        let not_found_server = MockServer::start().await;
        let not_found_client =
            GitHubPublicRepositoryClient::with_base(&format!("{}/", not_found_server.uri()))
                .expect("mock client");
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&not_found_server)
            .await;
        assert_eq!(
            not_found_client.fetch(&repository).await,
            Err(FetchError::NotFound)
        );
    }
}
