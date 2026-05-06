use std::time::Duration;

use anyhow::{Context, Result, ensure};
use reqwest::StatusCode;
use serde::Deserialize;

pub const DEFAULT_WEB_SEARCH_LIMIT: usize = 5;

const DEFAULT_WEB_SEARCH_URL: &str = "https://api.duckduckgo.com/";
const DEFAULT_WEB_SEARCH_TIMEOUT_SECONDS: u64 = 10;

#[derive(Clone)]
pub struct WebSearchClient {
    client: reqwest::Client,
    url: String,
    api_key: Option<String>,
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
struct DuckDuckGoResponse {
    #[serde(default, rename = "Heading")]
    heading: String,
    #[serde(default, rename = "AbstractText")]
    abstract_text: String,
    #[serde(default, rename = "AbstractURL")]
    abstract_url: String,
    #[serde(default, rename = "Results")]
    results: Vec<DuckDuckGoTopic>,
    #[serde(default, rename = "RelatedTopics")]
    related_topics: Vec<DuckDuckGoTopic>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DuckDuckGoTopic {
    Section {
        #[serde(rename = "Topics")]
        topics: Vec<DuckDuckGoTopic>,
    },
    Topic {
        #[serde(default, rename = "Text")]
        text: String,
        #[serde(default, rename = "FirstURL")]
        first_url: String,
    },
}

impl WebSearchClient {
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("WEB_SEARCH_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_WEB_SEARCH_URL.to_string());
        let api_key = std::env::var("WEB_SEARCH_API_KEY")
            .or_else(|_| std::env::var("WEB_SEARCH_BEARER_TOKEN"))
            .ok()
            .filter(|value| !value.trim().is_empty());
        let client = reqwest::Client::builder()
            .timeout(web_search_timeout())
            .build()
            .context("failed to build web search HTTP client")?;

        Ok(Self {
            client,
            url,
            api_key,
        })
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<WebSearchResults> {
        let query = query.trim();
        ensure!(!query.is_empty(), "web search query cannot be empty");
        ensure!(limit > 0, "web search limit must be greater than zero");

        let mut request = self.client.get(&self.url).query(&[
            ("q", query),
            ("format", "json"),
            ("no_html", "1"),
            ("skip_disambig", "1"),
        ]);

        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("failed to request web search for `{query}`"))?;
        let status = response.status();
        ensure_success_status(status)?;
        let response = response
            .json::<DuckDuckGoResponse>()
            .await
            .context("failed to parse web search response")?;

        Ok(parse_duckduckgo_results(query, response, limit))
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

fn parse_duckduckgo_results(
    query: &str,
    response: DuckDuckGoResponse,
    limit: usize,
) -> WebSearchResults {
    let mut results = Vec::new();

    push_result(
        &mut results,
        response.heading.trim(),
        response.abstract_url.trim(),
        response.abstract_text.trim(),
    );

    for topic in response
        .results
        .iter()
        .chain(response.related_topics.iter())
    {
        push_topic_results(&mut results, topic);
    }

    deduplicate_results(&mut results);
    results.truncate(limit);

    WebSearchResults {
        query: query.to_string(),
        results,
    }
}

fn push_topic_results(results: &mut Vec<SearchResult>, topic: &DuckDuckGoTopic) {
    match topic {
        DuckDuckGoTopic::Topic { text, first_url } => {
            push_result(results, topic_title(text), first_url.trim(), text.trim());
        }
        DuckDuckGoTopic::Section { topics } => {
            for topic in topics {
                push_topic_results(results, topic);
            }
        }
    }
}

fn push_result(results: &mut Vec<SearchResult>, title: &str, url: &str, snippet: &str) {
    if url.is_empty() || snippet.is_empty() {
        return;
    }

    results.push(SearchResult {
        title: fallback_title(title, snippet),
        url: url.to_string(),
        snippet: snippet.to_string(),
    });
}

fn fallback_title(title: &str, snippet: &str) -> String {
    if !title.is_empty() {
        return title.to_string();
    }

    topic_title(snippet).to_string()
}

fn topic_title(text: &str) -> &str {
    text.split(" - ").next().unwrap_or(text).trim()
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
    fn parses_abstract_and_related_topics() {
        let response: DuckDuckGoResponse = serde_json::from_str(
            r#"{
                "Heading": "Rust",
                "AbstractText": "Rust is a programming language.",
                "AbstractURL": "https://example.com/rust",
                "Results": [],
                "RelatedTopics": [
                    {
                        "Text": "Cargo - Rust package manager",
                        "FirstURL": "https://example.com/cargo"
                    },
                    {
                        "Topics": [
                            {
                                "Text": "Ratatui - terminal UI library",
                                "FirstURL": "https://example.com/ratatui"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        let results = parse_duckduckgo_results("rust", response, 3);

        assert_eq!(
            results.results,
            vec![
                SearchResult {
                    title: "Rust".to_string(),
                    url: "https://example.com/rust".to_string(),
                    snippet: "Rust is a programming language.".to_string(),
                },
                SearchResult {
                    title: "Cargo".to_string(),
                    url: "https://example.com/cargo".to_string(),
                    snippet: "Cargo - Rust package manager".to_string(),
                },
                SearchResult {
                    title: "Ratatui".to_string(),
                    url: "https://example.com/ratatui".to_string(),
                    snippet: "Ratatui - terminal UI library".to_string(),
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
}
