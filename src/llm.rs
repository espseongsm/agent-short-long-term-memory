use std::{fmt, time::Duration};

use agent_memory::{ChatEntry, ChatRole};
use anyhow::{Context, Result, bail};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequest, CreateChatCompletionRequestArgs,
        ReasoningEffort as OpenAiReasoningEffort,
    },
};
use clap::ValueEnum;

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

    fn to_openai(self) -> OpenAiReasoningEffort {
        match self {
            Self::None => OpenAiReasoningEffort::None,
            Self::Minimal => OpenAiReasoningEffort::Minimal,
            Self::Low => OpenAiReasoningEffort::Low,
            Self::Medium => OpenAiReasoningEffort::Medium,
            Self::High => OpenAiReasoningEffort::High,
            Self::Xhigh => OpenAiReasoningEffort::Xhigh,
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

use rig::{
    OneOrMany,
    completion::{
        AssistantContent, Message as RigMessage,
        message::{Text, UserContent},
    },
};

#[derive(Clone)]
pub struct LlmClient {
    client: Client<OpenAIConfig>,
    model: String,
    preamble: &'static str,
    reasoning_effort: Option<ReasoningEffort>,
}

impl LlmClient {
    pub fn from_env(
        model: String,
        preamble: &'static str,
        reasoning_effort: Option<ReasoningEffort>,
    ) -> Self {
        let mut config = OpenAIConfig::new();
        let openai_api_key = std::env::var("OPENAI_API_KEY").ok();

        if let Some(api_key) = openai_api_key
            .as_deref()
            .filter(|value| !looks_like_url(value))
            .map(ToOwned::to_owned)
            .or_else(|| std::env::var("BEARER_TOKEN").ok())
        {
            config = config.with_api_key(api_key);
        }

        if let Some(api_base) = std::env::var("OPENAI_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_API_BASE"))
            .ok()
            .or_else(|| openai_api_key.filter(|value| looks_like_url(value)))
        {
            config = config.with_api_base(api_base);
        }

        Self {
            client: Client::with_config(config),
            model,
            preamble,
            reasoning_effort,
        }
    }

    pub async fn chat(&self, history: &[ChatEntry], prompt: &str) -> Result<String> {
        let workflow_messages = rig_workflow_messages(self.preamble, history, prompt);
        let request =
            chat_completion_request(&self.model, &workflow_messages, self.reasoning_effort)?;
        let response = tokio::time::timeout(chat_timeout(), self.client.chat().create(request))
            .await
            .context("chat completion timed out")?
            .context("failed to create chat completion")?;

        response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .context("chat completion response did not include assistant text")
    }
}

fn chat_completion_request(
    model: &str,
    workflow_messages: &[RigMessage],
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<CreateChatCompletionRequest> {
    let mut request = CreateChatCompletionRequestArgs::default();
    request
        .model(model)
        .messages(openai_messages(workflow_messages)?);

    if let Some(reasoning_effort) = reasoning_effort {
        request.reasoning_effort(reasoning_effort.to_openai());
    }

    request
        .build()
        .context("failed to build chat completion request")
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
    let mut messages = vec![RigMessage::System {
        content: preamble.to_string(),
    }];

    for entry in history {
        messages.push(rig_chat_entry_message(entry));
    }

    messages.push(RigMessage::User {
        content: OneOrMany::one(UserContent::text(prompt)),
    });

    messages
}

fn rig_chat_entry_message(entry: &ChatEntry) -> RigMessage {
    match entry.role {
        ChatRole::User => RigMessage::User {
            content: OneOrMany::one(UserContent::text(entry.content.clone())),
        },
        ChatRole::Assistant => RigMessage::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::text(entry.content.clone())),
        },
    }
}

fn openai_messages(messages: &[RigMessage]) -> Result<Vec<ChatCompletionRequestMessage>> {
    messages.iter().map(openai_message).collect()
}

fn openai_message(message: &RigMessage) -> Result<ChatCompletionRequestMessage> {
    match message {
        RigMessage::System { content } => Ok(ChatCompletionRequestSystemMessageArgs::default()
            .content(content.clone())
            .build()
            .context("failed to build system message")?
            .into()),
        RigMessage::User { content } => Ok(ChatCompletionRequestUserMessageArgs::default()
            .content(rig_user_text(content))
            .build()
            .context("failed to build user message")?
            .into()),
        RigMessage::Assistant { content, .. } => {
            Ok(ChatCompletionRequestAssistantMessageArgs::default()
                .content(rig_assistant_text(content))
                .build()
                .context("failed to build assistant message")?
                .into())
        }
    }
}

fn rig_user_text(content: &OneOrMany<UserContent>) -> String {
    content
        .iter()
        .filter_map(|content| match content {
            UserContent::Text(text) => Some(text_to_string(text)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rig_assistant_text(content: &OneOrMany<AssistantContent>) -> String {
    content
        .iter()
        .filter_map(|content| match content {
            AssistantContent::Text(text) => Some(text_to_string(text)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_to_string(text: &Text) -> String {
    text.text.clone()
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
    fn openai_messages_are_built_from_rig_workflow_messages() {
        let history = vec![
            ChatEntry::new(ChatRole::User, "hello"),
            ChatEntry::new(ChatRole::Assistant, "hi"),
        ];
        let rig_messages = rig_workflow_messages("system", &history, "next");
        let messages = openai_messages(&rig_messages).unwrap();

        assert_eq!(messages.len(), 4);
        assert!(matches!(
            messages[0],
            ChatCompletionRequestMessage::System(_)
        ));
        assert!(matches!(messages[1], ChatCompletionRequestMessage::User(_)));
        assert!(matches!(
            messages[2],
            ChatCompletionRequestMessage::Assistant(_)
        ));
        assert!(matches!(messages[3], ChatCompletionRequestMessage::User(_)));
    }

    #[test]
    fn chat_request_omits_reasoning_effort_when_unset() {
        let rig_messages = rig_workflow_messages("system", &[], "next");
        let request = chat_completion_request("model", &rig_messages, None).unwrap();
        let value = serde_json::to_value(request).unwrap();

        assert!(value.get("reasoning_effort").is_none());
    }

    #[test]
    fn chat_request_includes_reasoning_effort_when_set() {
        let rig_messages = rig_workflow_messages("system", &[], "next");
        let request =
            chat_completion_request("model", &rig_messages, Some(ReasoningEffort::Low)).unwrap();
        let value = serde_json::to_value(request).unwrap();

        assert_eq!(value["reasoning_effort"], "low");
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
