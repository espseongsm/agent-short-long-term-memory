use crate::{
    agent_workflow,
    llm::{LlmClient, ReasoningEffort},
    web_search::{DEFAULT_WEB_SEARCH_LIMIT, WebSearchClient},
};
use agent_memory::{ChatEntry, ChatRole, ShortTermMemory};
use anyhow::{Context, Result};
use ratatui::DefaultTerminal;

use super::{
    render::{remember_agent_action, render, set_agent_action, show_local_entry},
    state::{App, TuiConfig},
};

pub(super) fn web_search_command_query(input: &str) -> Option<&str> {
    let input = input.trim();

    input
        .strip_prefix("/search")
        .or_else(|| input.strip_prefix("/web"))
        .map(str::trim)
}

pub(super) fn amplify_command_request(input: &str) -> Option<&str> {
    input.trim().strip_prefix("/amplify").map(str::trim)
}

pub(super) fn summary_command_value(input: &str) -> Option<&str> {
    let input = input.trim();

    input
        .strip_prefix("/summary")
        .or_else(|| input.strip_prefix("/summarize"))
        .map(str::trim)
}

pub(super) fn reasoning_effort_command_value(input: &str) -> Option<&str> {
    input
        .trim()
        .strip_prefix("/reasoning")
        .or_else(|| input.trim().strip_prefix("/effort"))
        .map(str::trim)
}

pub(super) fn copy_conversation_command_value(input: &str) -> Option<&str> {
    input.trim().strip_prefix("/copy").map(str::trim)
}

pub(super) fn parse_reasoning_effort_command(value: &str) -> Result<Option<ReasoningEffort>> {
    let value = value.trim();

    if value.eq_ignore_ascii_case("unset") || value.eq_ignore_ascii_case("default") {
        return Ok(None);
    }

    ReasoningEffort::from_env_value(Some(value))
}

pub(super) async fn submit_web_search(
    terminal: &mut DefaultTerminal,
    memory: &mut ShortTermMemory,
    web_search: &WebSearchClient,
    config: &TuiConfig,
    app: &mut App,
    prompt: String,
    query: &str,
) -> Result<()> {
    let user_entry =
        ChatEntry::for_session(&config.user_id, &config.session, ChatRole::User, &prompt);

    show_local_entry(app, user_entry.clone(), config.history_limit);
    app.status = "memory: saving web search request to Valkey".to_string();
    terminal.draw(|frame| render(frame, app, config))?;

    memory
        .append_chat_entry(
            &config.session,
            user_entry,
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save web search request")?;

    let action_index = remember_agent_action(
        app,
        app.entries.len(),
        "web search: querying Brave Search API",
    );
    app.status = format!("web search: searching `{query}`");
    terminal.draw(|frame| render(frame, app, config))?;

    let results = match web_search.search(query, DEFAULT_WEB_SEARCH_LIMIT).await {
        Ok(results) => results,
        Err(error) => {
            set_agent_action(app, action_index, "web search: request failed".to_string());
            return Err(error);
        }
    };

    set_agent_action(
        app,
        action_index,
        "web search: received results".to_string(),
    );
    let assistant_entry = ChatEntry::for_session(
        &config.user_id,
        &config.session,
        ChatRole::Assistant,
        results.to_markdown(),
    );

    memory
        .append_chat_entry(
            &config.session,
            assistant_entry.clone(),
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save web search results")?;

    show_local_entry(app, assistant_entry, config.history_limit);
    app.status = "ready".to_string();

    Ok(())
}

pub(super) async fn submit_request_amplification(
    terminal: &mut DefaultTerminal,
    memory: &mut ShortTermMemory,
    config: &TuiConfig,
    app: &mut App,
    prompt: String,
    request: &str,
) -> Result<()> {
    let user_entry =
        ChatEntry::for_session(&config.user_id, &config.session, ChatRole::User, &prompt);

    show_local_entry(app, user_entry.clone(), config.history_limit);
    app.status = "memory: saving amplification request to Valkey".to_string();
    terminal.draw(|frame| render(frame, app, config))?;

    memory
        .append_chat_entry(
            &config.session,
            user_entry,
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save amplification request")?;

    let action_index = remember_agent_action(
        app,
        app.entries.len(),
        "request amplifier: expanding user request",
    );
    app.status = "request amplifier: waiting for sub-agent".to_string();
    terminal.draw(|frame| render(frame, app, config))?;

    let request_amplifier = LlmClient::from_env(
        config.model.clone(),
        config.amplifier_preamble.clone(),
        app.reasoning_effort,
    )
    .context("failed to init request amplifier")?;
    let amplified = match request_amplifier.chat_with_usage(&[], request).await {
        Ok(response) => {
            app.token_usage.add(response.usage);
            response.text
        }
        Err(error) => {
            set_agent_action(
                app,
                action_index,
                "request amplifier: request failed".to_string(),
            );
            return Err(error);
        }
    };

    set_agent_action(
        app,
        action_index,
        "request amplifier: produced amplified request".to_string(),
    );
    let assistant_entry = ChatEntry::for_session(
        &config.user_id,
        &config.session,
        ChatRole::Assistant,
        amplified,
    );

    memory
        .append_chat_entry(
            &config.session,
            assistant_entry.clone(),
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save amplified request")?;

    show_local_entry(app, assistant_entry, config.history_limit);
    app.status = "ready".to_string();

    Ok(())
}

pub(super) async fn submit_conversation_summary(
    terminal: &mut DefaultTerminal,
    memory: &mut ShortTermMemory,
    config: &TuiConfig,
    app: &mut App,
    prompt: String,
) -> Result<()> {
    let history = memory
        .chat_history(&config.session)
        .context("failed to read chat history")?;
    let user_entry =
        ChatEntry::for_session(&config.user_id, &config.session, ChatRole::User, &prompt);

    show_local_entry(app, user_entry.clone(), config.history_limit);
    app.status = "memory: saving summary request to Valkey".to_string();
    terminal.draw(|frame| render(frame, app, config))?;

    memory
        .append_chat_entry(
            &config.session,
            user_entry,
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save summary request")?;

    if history.is_empty() {
        let assistant_entry = ChatEntry::for_session(
            &config.user_id,
            &config.session,
            ChatRole::Assistant,
            "No saved chat history to summarize yet.",
        );

        memory
            .append_chat_entry(
                &config.session,
                assistant_entry.clone(),
                config.history_limit,
                config.ttl_seconds,
            )
            .context("failed to save conversation summary")?;

        show_local_entry(app, assistant_entry, config.history_limit);
        app.status = "ready".to_string();
        return Ok(());
    }

    let action_index = remember_agent_action(
        app,
        app.entries.len(),
        "summary agent: summarizing conversation",
    );
    app.status = "summary agent: waiting for sub-agent".to_string();
    terminal.draw(|frame| render(frame, app, config))?;

    let summary = match agent_workflow::summarize_chat_history_with_usage(
        &history,
        config.model.clone(),
        app.reasoning_effort,
        &config.summary_preamble,
    )
    .await
    {
        Ok(response) => {
            app.token_usage.add(response.usage);
            response.text
        }
        Err(error) => {
            set_agent_action(
                app,
                action_index,
                "summary agent: request failed".to_string(),
            );
            return Err(error);
        }
    };

    set_agent_action(
        app,
        action_index,
        "summary agent: produced conversation summary".to_string(),
    );
    let assistant_entry = ChatEntry::for_session(
        &config.user_id,
        &config.session,
        ChatRole::Assistant,
        summary,
    );

    memory
        .append_chat_entry(
            &config.session,
            assistant_entry.clone(),
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save conversation summary")?;

    show_local_entry(app, assistant_entry, config.history_limit);
    app.status = "ready".to_string();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_web_search_commands() {
        assert_eq!(
            web_search_command_query("/search rust tui"),
            Some("rust tui")
        );
        assert_eq!(web_search_command_query("/web valkey"), Some("valkey"));
        assert_eq!(web_search_command_query("hello"), None);
    }

    #[test]
    fn parses_amplify_commands() {
        assert_eq!(
            amplify_command_request("/amplify build a tui"),
            Some("build a tui")
        );
        assert_eq!(amplify_command_request("build a tui"), None);
    }

    #[test]
    fn parses_summary_commands() {
        assert_eq!(summary_command_value("/summary"), Some(""));
        assert_eq!(summary_command_value("/summarize all"), Some("all"));
        assert_eq!(summary_command_value("hello"), None);
    }

    #[test]
    fn parses_reasoning_effort_commands() {
        assert_eq!(
            reasoning_effort_command_value("/reasoning low"),
            Some("low")
        );
        assert_eq!(
            reasoning_effort_command_value("/effort unset"),
            Some("unset")
        );
        assert_eq!(reasoning_effort_command_value("hello"), None);
    }

    #[test]
    fn parses_copy_conversation_command() {
        assert_eq!(copy_conversation_command_value("/copy"), Some(""));
        assert_eq!(copy_conversation_command_value("/copy all"), Some("all"));
        assert_eq!(copy_conversation_command_value("hello"), None);
    }

    #[test]
    fn parses_reasoning_effort_command_values() {
        assert_eq!(
            parse_reasoning_effort_command("low").unwrap(),
            Some(ReasoningEffort::Low)
        );
        assert_eq!(parse_reasoning_effort_command("unset").unwrap(), None);
        assert!(parse_reasoning_effort_command("fast").is_err());
    }
}
