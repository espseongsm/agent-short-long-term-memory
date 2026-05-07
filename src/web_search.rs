use std::time::Duration;

use anyhow::{Context, Result, ensure};
use reqwest::{StatusCode, header};
use serde::Deserialize;

pub const DEFAULT_WEB_SEARCH_LIMIT: usize = 5;

const DEFAULT_WEB_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const DEFAULT_WEB_SEARCH_TIMEOUT_SECONDS: u64 = 10;

#[derive(Clone)]
pub struct WebSearchClient {
    client: reqwest::Client,
    url: String,
    auth: Option<WebSearchAuth>,
}

#[derive(Clone)]
enum WebSearchAuth {
    SubscriptionToken(String),
    BearerToken(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebSearchResults {
    query: String,
    results: Vec<SearchResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    #[serde(default)]
    results: Vec<BraveSearchResult>,
}

#[derive(Debug, Deserialize)]
struct BraveSearchResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    extra_snippets: Vec<String>,
}

impl WebSearchClient {
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("WEB_SEARCH_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_WEB_SEARCH_URL.to_string());
        let auth = web_search_auth();
        let client = reqwest::Client::builder()
            .timeout(web_search_timeout())
            .build()
            .context("failed to build web search HTTP client")?;

        Ok(Self { client, url, auth })
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<WebSearchResults> {
        let query = query.trim();
        ensure!(!query.is_empty(), "web search query cannot be empty");
        ensure!(limit > 0, "web search limit must be greater than zero");

        let count = limit.min(20).to_string();
        let mut request = self
            .client
            .get(&self.url)
            .query(&[
                ("q", query),
                ("count", count.as_str()),
                ("extra_snippets", "true"),
            ])
            .header(header::ACCEPT, "application/json");

        match &self.auth {
            Some(WebSearchAuth::SubscriptionToken(api_key)) => {
                request = request.header("X-Subscription-Token", api_key);
            }
            Some(WebSearchAuth::BearerToken(api_key)) => {
                request = request.bearer_auth(api_key);
            }
            None => {
                ensure!(
                    !is_default_brave_url(&self.url),
                    "BRAVE_SEARCH_API_KEY or WEB_SEARCH_API_KEY must be set for Brave Search"
                );
            }
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("failed to request web search for `{query}`"))?;
        let status = response.status();
        ensure_success_status(status)?;
        let response = response
            .json::<BraveSearchResponse>()
            .await
            .context("failed to parse Brave Search response")?;

        Ok(parse_brave_results(query, response, limit))
    }
}

impl WebSearchResults {
    pub fn to_markdown(&self) -> String {
        if self.results.is_empty() {
            return format!("No web search results for `{}`.", self.query);
        }

        let mut markdown = format!("Web search results for `{}`:", self.query);

        for (index, result) in self.results.iter().enumerate() {
            markdown.push_str(&format!(
                "\n\n{}. [{}]({})\n   {}",
                index + 1,
                result.title,
                result.url,
                result.snippet
            ));
        }

        markdown
    }
}

fn web_search_auth() -> Option<WebSearchAuth> {
    std::env::var("BRAVE_SEARCH_API_KEY")
        .or_else(|_| std::env::var("WEB_SEARCH_API_KEY"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(WebSearchAuth::SubscriptionToken)
        .or_else(|| {
            std::env::var("WEB_SEARCH_BEARER_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(WebSearchAuth::BearerToken)
        })
}

fn parse_brave_results(
    query: &str,
    response: BraveSearchResponse,
    limit: usize,
) -> WebSearchResults {
    let mut results = response
        .web
        .map(|web| web.results)
        .unwrap_or_default()
        .into_iter()
        .filter_map(search_result_from_brave)
        .collect::<Vec<_>>();

    deduplicate_results(&mut results);
    results.truncate(limit);

    WebSearchResults {
        query: query.to_string(),
        results,
    }
}

fn search_result_from_brave(result: BraveSearchResult) -> Option<SearchResult> {
    let title = result.title.trim();
    let url = result.url.trim();
    let snippet = brave_snippet(&result);

    if title.is_empty() || url.is_empty() || snippet.is_empty() {
        return None;
    }

    Some(SearchResult {
        title: title.to_string(),
        url: url.to_string(),
        snippet,
    })
}

fn brave_snippet(result: &BraveSearchResult) -> String {
    std::iter::once(result.description.trim())
        .chain(result.extra_snippets.iter().map(|snippet| snippet.trim()))
        .filter(|snippet| !snippet.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn deduplicate_results(results: &mut Vec<SearchResult>) {
    let mut seen_urls = Vec::new();

    results.retain(|result| {
        if seen_urls.contains(&result.url) {
            return false;
        }

        seen_urls.push(result.url.clone());
        true
    });
}

fn ensure_success_status(status: StatusCode) -> Result<()> {
    ensure!(
        status.is_success(),
        "web search request failed with HTTP status {status}"
    );

    Ok(())
}

fn is_default_brave_url(url: &str) -> bool {
    url.trim_end_matches('/') == DEFAULT_WEB_SEARCH_URL
}

fn web_search_timeout() -> Duration {
    std::env::var("WEB_SEARCH_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_WEB_SEARCH_TIMEOUT_SECONDS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brave_web_results() {
        let response: BraveSearchResponse = serde_json::from_str(
            r#"{
                "web": {
                    "results": [
                        {
                            "title": "Rust",
                            "url": "https://example.com/rust",
                            "description": "Rust is a programming language.",
                            "extra_snippets": [
                                "Cargo is Rust's package manager."
                            ]
                        },
                        {
                            "title": "Rust duplicate",
                            "url": "https://example.com/rust",
                            "description": "Duplicate result."
                        },
                        {
                            "title": "Ratatui",
                            "url": "https://example.com/ratatui",
                            "description": "Ratatui is a terminal UI library."
                        }
                    ]
                }
            }"#,
        )
        .unwrap();

        let results = parse_brave_results("rust", response, 3);

        assert_eq!(
            results.results,
            vec![
                SearchResult {
                    title: "Rust".to_string(),
                    url: "https://example.com/rust".to_string(),
                    snippet: "Rust is a programming language. Cargo is Rust's package manager."
                        .to_string(),
                },
                SearchResult {
                    title: "Ratatui".to_string(),
                    url: "https://example.com/ratatui".to_string(),
                    snippet: "Ratatui is a terminal UI library.".to_string(),
                },
            ]
        );
    }

    #[test]
    fn formats_empty_search_results() {
        let results = WebSearchResults {
            query: "nothing".to_string(),
            results: Vec::new(),
        };

        assert_eq!(
            results.to_markdown(),
            "No web search results for `nothing`."
        );
    }

    #[test]
    fn detects_default_brave_url_with_trailing_slash() {
        assert!(is_default_brave_url(
            "https://api.search.brave.com/res/v1/web/search/"
        ));
        assert!(!is_default_brave_url("https://example.com/search"));
    }
}
