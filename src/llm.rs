use std::{fmt, time::Duration};

use agent_memory::{ChatEntry, ChatRole};
use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use rig::{
    OneOrMany,
    completion::{
        AssistantContent, CompletionModel as _, CompletionRequest, Message as RigMessage,
    },
    prelude::CompletionClient,
    providers::openai,
};
use serde_json::json;

const DEFAULT_CHAT_TIMEOUT_SECONDS: u64 = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffort {
    pub fn from_env_value(value: Option<&str>) -> Result<Option<Self>> {
        value.map(Self::parse).transpose()
    }

    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "minimal" => Ok(Self::Minimal),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" => Ok(Self::Xhigh),
            _ => bail!(
                "LLM_REASONING_EFFORT must be one of: none, minimal, low, medium, high, xhigh"
            ),
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        };

        formatter.write_str(value)
    }
}

#[derive(Clone)]
pub struct LlmClient {
    model: openai::CompletionModel,
    preamble: &'static str,
    reasoning_effort: Option<ReasoningEffort>,
}

impl LlmClient {
    pub fn from_env(
        model: String,
        preamble: &'static str,
        reasoning_effort: Option<ReasoningEffort>,
    ) -> Result<Self> {
        let api_key =
            llm_api_key().context("OPENAI_API_KEY or BEARER_TOKEN must be set for model calls")?;
        let mut builder = openai::CompletionsClient::builder().api_key(api_key);

        if let Some(api_base) = llm_api_base() {
            builder = builder.base_url(api_base);
        }

        let client = builder
            .build()
            .context("failed to build Rig OpenAI-compatible client")?;

        Ok(Self {
            model: client.completion_model(model),
            preamble,
            reasoning_effort,
        })
    }

    pub async fn chat(&self, history: &[ChatEntry], prompt: &str) -> Result<String> {
        let workflow_messages = rig_workflow_messages(self.preamble, history, prompt);
        let request = rig_completion_request(workflow_messages, self.reasoning_effort)?;
        let response = tokio::time::timeout(chat_timeout(), self.model.completion(request))
            .await
            .context("chat completion timed out")?
            .context("failed to create chat completion through Rig")?;

        assistant_text(&response.choice).context("chat completion response did not include text")
    }
}

fn llm_api_key() -> Option<String> {
    let openai_api_key = std::env::var("OPENAI_API_KEY").ok();

    openai_api_key
        .as_deref()
        .filter(|value| !looks_like_url(value))
        .map(ToOwned::to_owned)
        .or_else(|| std::env::var("BEARER_TOKEN").ok())
        .filter(|value| !value.trim().is_empty())
}

fn llm_api_base() -> Option<String> {
    std::env::var("OPENAI_BASE_URL")
        .or_else(|_| std::env::var("OPENAI_API_BASE"))
        .ok()
        .or_else(|| {
            std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|value| looks_like_url(value))
        })
        .filter(|value| !value.trim().is_empty())
}

fn rig_completion_request(
    workflow_messages: Vec<RigMessage>,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<CompletionRequest> {
    Ok(CompletionRequest {
        model: None,
        preamble: None,
        chat_history: OneOrMany::many(workflow_messages)
            .map_err(|_| anyhow!("chat completion requires at least one Rig message"))?,
        documents: Vec::new(),
        tools: Vec::new(),
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: reasoning_effort_params(reasoning_effort),
        output_schema: None,
    })
}

fn reasoning_effort_params(reasoning_effort: Option<ReasoningEffort>) -> Option<serde_json::Value> {
    reasoning_effort.map(|reasoning_effort| {
        json!({
            "reasoning_effort": reasoning_effort.to_string()
        })
    })
}

fn assistant_text(choice: &OneOrMany<AssistantContent>) -> Option<String> {
    let text = choice
        .iter()
        .filter_map(|content| match content {
            AssistantContent::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if text.is_empty() { None } else { Some(text) }
}

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn chat_timeout() -> Duration {
    chat_timeout_from_value(std::env::var("LLM_TIMEOUT_SECONDS").ok().as_deref())
}

fn chat_timeout_from_value(value: Option<&str>) -> Duration {
    value
        .and_then(|value| value.parse().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_CHAT_TIMEOUT_SECONDS))
}

fn rig_workflow_messages(preamble: &str, history: &[ChatEntry], prompt: &str) -> Vec<RigMessage> {
    let mut messages = vec![RigMessage::system(preamble)];

    for entry in history {
        messages.push(rig_chat_entry_message(entry));
    }

    messages.push(RigMessage::user(prompt));
    messages
}

fn rig_chat_entry_message(entry: &ChatEntry) -> RigMessage {
    match entry.role {
        ChatRole::User => RigMessage::user(entry.content.clone()),
        ChatRole::Assistant => RigMessage::assistant(entry.content.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_messages_include_preamble_history_and_prompt() {
        let history = vec![
            ChatEntry::new(ChatRole::User, "hello"),
            ChatEntry::new(ChatRole::Assistant, "hi"),
        ];

        let messages = rig_workflow_messages("system", &history, "next");

        assert_eq!(messages.len(), 4);
        assert!(matches!(messages[0], RigMessage::System { .. }));
        assert!(matches!(messages[1], RigMessage::User { .. }));
        assert!(matches!(messages[2], RigMessage::Assistant { .. }));
        assert!(matches!(messages[3], RigMessage::User { .. }));
    }

    #[test]
    fn rig_completion_request_omits_reasoning_effort_when_unset() {
        let messages = rig_workflow_messages("system", &[], "next");
        let request = rig_completion_request(messages, None).unwrap();

        assert!(request.additional_params.is_none());
    }

    #[test]
    fn rig_completion_request_includes_reasoning_effort_when_set() {
        let messages = rig_workflow_messages("system", &[], "next");
        let request = rig_completion_request(messages, Some(ReasoningEffort::Low)).unwrap();

        assert_eq!(
            request.additional_params.unwrap()["reasoning_effort"],
            "low"
        );
    }

    #[test]
    fn extracts_assistant_text_from_rig_response_choice() {
        let choice = OneOrMany::many(vec![
            AssistantContent::text("hello"),
            AssistantContent::text("world"),
        ])
        .unwrap();

        assert_eq!(assistant_text(&choice), Some("hello\nworld".to_string()));
    }

    #[test]
    fn parses_reasoning_effort_from_env_value() {
        assert_eq!(
            ReasoningEffort::from_env_value(Some("LOW")).unwrap(),
            Some(ReasoningEffort::Low)
        );
        assert_eq!(ReasoningEffort::from_env_value(None).unwrap(), None);
        assert!(ReasoningEffort::from_env_value(Some("fast")).is_err());
    }

    #[test]
    fn detects_url_like_values() {
        assert!(looks_like_url("https://example.com/v1"));
        assert!(looks_like_url("http://example.com/v1"));
        assert!(!looks_like_url("sk-example"));
    }

    #[test]
    fn chat_timeout_uses_default_when_value_is_missing_or_invalid() {
        assert_eq!(
            chat_timeout_from_value(None),
            Duration::from_secs(DEFAULT_CHAT_TIMEOUT_SECONDS)
        );
        assert_eq!(
            chat_timeout_from_value(Some("0")),
            Duration::from_secs(DEFAULT_CHAT_TIMEOUT_SECONDS)
        );
        assert_eq!(
            chat_timeout_from_value(Some("abc")),
            Duration::from_secs(DEFAULT_CHAT_TIMEOUT_SECONDS)
        );
    }

    #[test]
    fn chat_timeout_uses_positive_value() {
        assert_eq!(chat_timeout_from_value(Some("5")), Duration::from_secs(5));
    }
}
