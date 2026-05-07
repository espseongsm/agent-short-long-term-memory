use crate::{
    agent_workflow, llm::LlmClient, long_term_memory, web_search::DEFAULT_WEB_SEARCH_LIMIT,
};
use agent_memory::ChatEntry;
use anyhow::Result;
use ratatui::DefaultTerminal;

use super::{
    render::{format_error_chain, remember_agent_action, render, set_agent_action},
    state::{AgentServices, App, TuiConfig},
};

pub(super) async fn prepare_automatic_chat_prompt(
    terminal: &mut DefaultTerminal,
    services: AgentServices<'_>,
    config: &TuiConfig,
    app: &mut App,
    history: &[ChatEntry],
    prompt: &str,
) -> Result<String> {
    let actions = agent_workflow::automatic_actions_for_chat(history, prompt);
    let mut prompt_for_llm = prompt.to_string();

    if actions.amplify {
        let action_index = remember_agent_action(
            app,
            app.entries.len(),
            "request amplifier: automatically expanding vague request",
        );
        app.status = "request amplifier: automatically expanding vague request".to_string();
        terminal.draw(|frame| render(frame, app, config))?;

        let request_amplifier = LlmClient::from_env(
            config.model.clone(),
            config.amplifier_preamble.clone(),
            app.reasoning_effort,
        );
        match request_amplifier {
            Ok(request_amplifier) => match request_amplifier.chat_with_usage(history, prompt).await
            {
                Ok(response) => {
                    app.token_usage.add(response.usage);
                    set_agent_action(
                        app,
                        action_index,
                        "request amplifier: produced automatic amplified request".to_string(),
                    );
                    prompt_for_llm =
                        agent_workflow::prompt_with_amplification(prompt, &response.text);
                }
                Err(error) => {
                    set_agent_action(
                        app,
                        action_index,
                        "request amplifier: automatic request failed; continuing with original request"
                            .to_string(),
                    );
                    app.status = format_error_chain(&error);
                }
            },
            Err(error) => {
                set_agent_action(
                    app,
                    action_index,
                    "request amplifier: automatic init failed; continuing with original request"
                        .to_string(),
                );
                app.status = format_error_chain(&error);
            }
        }

        terminal.draw(|frame| render(frame, app, config))?;
    }

    if actions.weather {
        let location = agent_workflow::weather_location_query_for_chat(history, prompt);
        let action_index = remember_agent_action(
            app,
            app.entries.len(),
            "weather: automatically fetching current conditions",
        );
        app.status = format!("weather: fetching current conditions for `{location}`");
        terminal.draw(|frame| render(frame, app, config))?;

        match services.weather.current_weather(&location).await {
            Ok(report) => {
                set_agent_action(
                    app,
                    action_index,
                    "weather: added automatic current weather context".to_string(),
                );
                prompt_for_llm = agent_workflow::prompt_with_weather_context(
                    &prompt_for_llm,
                    &report.to_markdown(),
                );
            }
            Err(error) => {
                set_agent_action(
                    app,
                    action_index,
                    "weather: direct lookup failed; translating location for geocoding".to_string(),
                );
                app.status =
                    "weather: direct lookup failed; translating location for geocoding".to_string();
                terminal.draw(|frame| render(frame, app, config))?;

                match agent_workflow::translate_weather_location_to_english(
                    &location,
                    &config.model,
                    app.reasoning_effort,
                    &config.weather_translator_preamble,
                )
                .await
                {
                    Ok(Some(translated_location)) => {
                        set_agent_action(
                            app,
                            action_index,
                            format!(
                                "weather: retrying current conditions for `{translated_location}`"
                            ),
                        );
                        app.status = format!(
                            "weather: retrying current conditions for `{translated_location}`"
                        );
                        terminal.draw(|frame| render(frame, app, config))?;

                        match services.weather.current_weather(&translated_location).await {
                            Ok(report) => {
                                set_agent_action(
                                    app,
                                    action_index,
                                    "weather: added automatic current weather context via translated location"
                                        .to_string(),
                                );
                                prompt_for_llm = agent_workflow::prompt_with_weather_context(
                                    &prompt_for_llm,
                                    &report.to_markdown(),
                                );
                            }
                            Err(error) => {
                                set_agent_action(
                                    app,
                                    action_index,
                                    "weather: automatic request failed; continuing without weather context"
                                        .to_string(),
                                );
                                app.status = format_error_chain(&error.context(format!(
                                    "translated weather lookup for `{translated_location}` failed"
                                )));
                            }
                        }
                    }
                    Ok(None) => {
                        set_agent_action(
                            app,
                            action_index,
                            "weather: automatic request failed; continuing without weather context"
                                .to_string(),
                        );
                        app.status = format_error_chain(&error);
                    }
                    Err(error) => {
                        set_agent_action(
                            app,
                            action_index,
                            "weather: automatic request failed; continuing without weather context"
                                .to_string(),
                        );
                        app.status = format_error_chain(
                            &error.context("weather location translation failed"),
                        );
                    }
                }
            }
        }

        terminal.draw(|frame| render(frame, app, config))?;
    }

    if actions.web_search {
        let action_index = remember_agent_action(
            app,
            app.entries.len(),
            "web search: automatically searching for current context",
        );
        app.status = "web search: automatically searching for current context".to_string();
        terminal.draw(|frame| render(frame, app, config))?;

        match services
            .web_search
            .search(prompt, DEFAULT_WEB_SEARCH_LIMIT)
            .await
        {
            Ok(results) => {
                set_agent_action(
                    app,
                    action_index,
                    "web search: added automatic web context".to_string(),
                );
                prompt_for_llm = agent_workflow::prompt_with_web_search_context(
                    &prompt_for_llm,
                    &results.to_markdown(),
                );
            }
            Err(error) => {
                set_agent_action(
                    app,
                    action_index,
                    "web search: automatic request failed; continuing without web context"
                        .to_string(),
                );
                app.status = format_error_chain(&error);
            }
        }

        terminal.draw(|frame| render(frame, app, config))?;
    }

    if let Some(pgvector_url) = config.pgvector_url.as_deref() {
        let action_index = remember_agent_action(
            app,
            app.entries.len(),
            "long-term memory: searching local Markdown context",
        );
        app.status = "long-term memory: searching local Markdown context".to_string();
        terminal.draw(|frame| render(frame, app, config))?;

        match long_term_memory::connect(pgvector_url).await {
            Ok(long_term) => match long_term.search(prompt, 3).await {
                Ok(results) if !results.is_empty() => {
                    set_agent_action(
                        app,
                        action_index,
                        "long-term memory: added local Markdown context".to_string(),
                    );
                    prompt_for_llm = agent_workflow::prompt_with_long_term_context(
                        &prompt_for_llm,
                        &long_term_memory::LongTermSearchResult::to_markdown(&results),
                    );
                }
                Ok(_) => {
                    set_agent_action(
                        app,
                        action_index,
                        "long-term memory: no relevant local Markdown context found".to_string(),
                    );
                }
                Err(error) => {
                    set_agent_action(
                        app,
                        action_index,
                        "long-term memory: search failed; continuing without local context"
                            .to_string(),
                    );
                    app.status = format_error_chain(&error);
                }
            },
            Err(error) => {
                set_agent_action(
                    app,
                    action_index,
                    "long-term memory: connection failed; continuing without local context"
                        .to_string(),
                );
                app.status = format_error_chain(&error);
            }
        }

        terminal.draw(|frame| render(frame, app, config))?;
    }

    Ok(prompt_for_llm)
}
