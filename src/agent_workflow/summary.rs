use crate::llm::{LlmClient, LlmResponse, ReasoningEffort};
use agent_memory::ChatEntry;
use anyhow::{Context, Result};

#[allow(dead_code)]
pub async fn summarize_chat_history(
    history: &[ChatEntry],
    model: String,
    reasoning_effort: Option<ReasoningEffort>,
    preamble: &str,
) -> Result<String> {
    Ok(
        summarize_chat_history_with_usage(history, model, reasoning_effort, preamble)
            .await?
            .text,
    )
}

pub async fn summarize_chat_history_with_usage(
    history: &[ChatEntry],
    model: String,
    reasoning_effort: Option<ReasoningEffort>,
    preamble: &str,
) -> Result<LlmResponse> {
    let summarizer = LlmClient::from_env(model, preamble, reasoning_effort)
        .context("failed to init summary agent")?;

    summarizer
        .chat_with_usage(&[], &summary_prompt(history))
        .await
        .context("failed to summarize chat history")
}

pub fn summary_prompt(history: &[ChatEntry]) -> String {
    let transcript = history
        .iter()
        .map(|entry| format!("{}: {}", entry.role, entry.content.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "Summarize this chat history for future continuation.\n\
         Preserve important user preferences, decisions, unresolved questions, and next steps.\n\
         Keep it concise and do not invent details.\n\n\
         Chat history:\n{transcript}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_memory::{ChatEntry, ChatRole};

    #[test]
    fn summary_prompt_formats_chat_history() {
        let prompt = summary_prompt(&[
            ChatEntry::new(ChatRole::User, "remember Valkey setup"),
            ChatEntry::new(ChatRole::Assistant, "Valkey is running locally."),
        ]);

        assert!(prompt.contains("user: remember Valkey setup"));
        assert!(prompt.contains("assistant: Valkey is running locally."));
    }

    #[test]
    fn summary_prompt_trims_entry_content() {
        let prompt = summary_prompt(&[ChatEntry::new(ChatRole::User, "  hello  ")]);

        assert!(prompt.contains("user: hello"));
        assert!(!prompt.contains("  hello  "));
    }
}
