use std::{
    io,
    time::{Duration, Instant},
};

use crate::{agent_workflow, llm::LlmClient, weather::WeatherClient, web_search::WebSearchClient};
use agent_memory::{ChatEntry, ChatRole, ShortTermMemory};
use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
};
use ratatui::{DefaultTerminal, layout::Rect};

use super::{
    commands::{
        amplify_command_request, copy_conversation_command_value, parse_reasoning_effort_command,
        reasoning_effort_command_value, submit_conversation_summary, submit_request_amplification,
        submit_web_search, summary_command_value, web_search_command_query,
    },
    context::prepare_automatic_chat_prompt,
    interaction::{
        copy_conversation_to_clipboard, copy_prompt_to_clipboard, handle_mouse_event,
        paste_text_from_clipboard, push_pasted_text,
    },
    render::{
        format_elapsed_time, format_error_chain, reasoning_effort_label, remember_agent_action,
        render, scroll_conversation_down, scroll_conversation_to_bottom,
        scroll_conversation_to_top, scroll_conversation_up, set_agent_action, show_local_entry,
        token_usage_label,
    },
    state::{AgentServices, App, PendingResponse, TuiConfig},
};

pub async fn run(memory: &mut ShortTermMemory, config: TuiConfig) -> Result<()> {
    let weather = WeatherClient::from_env().context("failed to init weather")?;
    let web_search = WebSearchClient::from_env().context("failed to init web search")?;
    let history = memory
        .chat_history(&config.session)
        .context("failed to read chat history")?;
    let mut app = App::new(history, config.reasoning_effort);
    let mut terminal = ratatui::init();
    if let Err(error) = execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)
        .context("failed to enable terminal event capture")
    {
        ratatui::restore();
        return Err(error);
    }

    let result = run_app(
        &mut terminal,
        memory,
        AgentServices {
            weather: &weather,
            web_search: &web_search,
        },
        &config,
        &mut app,
    )
    .await;
    let terminal_event_result = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture)
        .context("failed to disable terminal event capture");
    ratatui::restore();

    terminal_event_result?;
    result
}

async fn run_app(
    terminal: &mut DefaultTerminal,
    memory: &mut ShortTermMemory,
    services: AgentServices<'_>,
    config: &TuiConfig,
    app: &mut App,
) -> Result<()> {
    loop {
        if app
            .pending_response
            .as_ref()
            .is_some_and(|pending| pending.handle.is_finished())
            && let Err(error) = finish_pending_response(memory, config, app).await
        {
            app.status = format_error_chain(&error);
        }

        terminal.draw(|frame| render(frame, app, config))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if app.input.is_empty() {
                            break;
                        }

                        if let Err(error) = copy_prompt_to_clipboard(&app.input) {
                            app.status = format_error_chain(&error);
                        } else {
                            app.status = "clipboard: copied prompt".to_string();
                        }
                    }
                    KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        match paste_text_from_clipboard() {
                            Ok(text) => {
                                push_pasted_text(&mut app.input, &text);
                                app.status = "clipboard: pasted text".to_string();
                            }
                            Err(error) => {
                                app.status = format_error_chain(&error);
                            }
                        }
                    }
                    KeyCode::Up => scroll_conversation_up(app, 1),
                    KeyCode::Down => scroll_conversation_down(app, 1),
                    KeyCode::PageUp => scroll_conversation_up(app, 8),
                    KeyCode::PageDown => scroll_conversation_down(app, 8),
                    KeyCode::Home => scroll_conversation_to_top(app),
                    KeyCode::End => scroll_conversation_to_bottom(app),
                    KeyCode::Char(character) => app.input.push(character),
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Enter => {
                        if app.pending_response.is_some() {
                            app.status = "LLM: waiting for the current response".to_string();
                            continue;
                        }

                        let prompt = app.input.trim().to_string();
                        if prompt.is_empty() {
                            continue;
                        }

                        if let Some(value) = reasoning_effort_command_value(&prompt) {
                            if value.is_empty() {
                                app.status =
                                    "usage: /reasoning <unset|none|minimal|low|medium|high|xhigh>"
                                        .to_string();
                                continue;
                            }

                            match parse_reasoning_effort_command(value) {
                                Ok(reasoning_effort) => {
                                    app.reasoning_effort = reasoning_effort;
                                    app.input.clear();
                                    app.status = format!(
                                        "reasoning effort: {}",
                                        reasoning_effort_label(app.reasoning_effort)
                                    );
                                }
                                Err(error) => app.status = format_error_chain(&error),
                            }

                            continue;
                        }

                        if let Some(value) = copy_conversation_command_value(&prompt) {
                            if !value.is_empty() && !value.eq_ignore_ascii_case("all") {
                                app.status = "usage: /copy".to_string();
                                continue;
                            }

                            match copy_conversation_to_clipboard(app, config) {
                                Ok(()) => {
                                    app.input.clear();
                                    app.status =
                                        "clipboard: copied conversation and status".to_string();
                                }
                                Err(error) => app.status = format_error_chain(&error),
                            }

                            continue;
                        }

                        if let Some(value) = summary_command_value(&prompt) {
                            if !value.is_empty() && !value.eq_ignore_ascii_case("all") {
                                app.status = "usage: /summary".to_string();
                                continue;
                            }

                            app.input.clear();

                            if let Err(error) =
                                submit_conversation_summary(terminal, memory, config, app, prompt)
                                    .await
                            {
                                app.status = format_error_chain(&error);
                            }

                            continue;
                        }

                        if agent_workflow::should_auto_summarize(&prompt) {
                            app.input.clear();

                            if let Err(error) =
                                submit_conversation_summary(terminal, memory, config, app, prompt)
                                    .await
                            {
                                app.status = format_error_chain(&error);
                            }

                            continue;
                        }

                        if let Some(query) = web_search_command_query(&prompt) {
                            if query.is_empty() {
                                app.status = "usage: /search <query>".to_string();
                                continue;
                            }

                            let query = query.to_string();
                            app.input.clear();

                            if let Err(error) = submit_web_search(
                                terminal,
                                memory,
                                services.web_search,
                                config,
                                app,
                                prompt,
                                &query,
                            )
                            .await
                            {
                                app.status = format_error_chain(&error);
                            }

                            continue;
                        }

                        if let Some(request) = amplify_command_request(&prompt) {
                            if request.is_empty() {
                                app.status = "usage: /amplify <request>".to_string();
                                continue;
                            }

                            let request = request.to_string();
                            app.input.clear();

                            if let Err(error) = submit_request_amplification(
                                terminal, memory, config, app, prompt, &request,
                            )
                            .await
                            {
                                app.status = format_error_chain(&error);
                            }

                            continue;
                        }

                        let history = match memory
                            .chat_history(&config.session)
                            .context("failed to read chat history")
                        {
                            Ok(history) => history,
                            Err(error) => {
                                app.status = format_error_chain(&error);
                                continue;
                            }
                        };
                        let user_entry = ChatEntry::for_session(
                            &config.user_id,
                            &config.session,
                            ChatRole::User,
                            &prompt,
                        );

                        app.input.clear();
                        show_local_entry(app, user_entry.clone(), config.history_limit);
                        app.status = "memory: saving user message to Valkey".to_string();
                        terminal.draw(|frame| render(frame, app, config))?;

                        if let Err(error) = memory
                            .append_chat_entry(
                                &config.session,
                                user_entry,
                                config.history_limit,
                                config.ttl_seconds,
                            )
                            .context("failed to save user chat history")
                        {
                            app.status = format_error_chain(&error);
                            continue;
                        }

                        let prompt_for_llm = match prepare_automatic_chat_prompt(
                            terminal, services, config, app, &history, &prompt,
                        )
                        .await
                        {
                            Ok(prompt_for_llm) => prompt_for_llm,
                            Err(error) => {
                                app.status = format_error_chain(&error);
                                continue;
                            }
                        };

                        if let Err(error) =
                            start_prompt_submission(config, app, history, prompt_for_llm)
                        {
                            app.status = format_error_chain(&error);
                        }
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => {
                let terminal_size = terminal.size()?;
                let terminal_area = Rect::new(0, 0, terminal_size.width, terminal_size.height);

                if let Err(error) = handle_mouse_event(app, terminal_area, mouse) {
                    app.status = format_error_chain(&error);
                }
            }
            Event::Paste(text) => {
                push_pasted_text(&mut app.input, &text);
                app.status = "clipboard: pasted text".to_string();
            }
            _ => {}
        }
    }

    if let Some(pending_response) = app.pending_response.take() {
        pending_response.handle.abort();
    }

    Ok(())
}

fn start_prompt_submission(
    config: &TuiConfig,
    app: &mut App,
    history: Vec<ChatEntry>,
    prompt: String,
) -> Result<()> {
    let llm = LlmClient::from_env(
        config.model.clone(),
        config.preamble.clone(),
        app.reasoning_effort,
    )
    .context("failed to init LLM client")?;
    let action_index = remember_agent_action(
        app,
        app.entries.len(),
        "LLM: Rig workflow -> OpenAI-compatible provider -> waiting for model",
    );
    let handle = tokio::spawn(async move { llm.chat_with_usage(&history, &prompt).await });

    app.pending_response = Some(PendingResponse {
        handle,
        started_at: Instant::now(),
        action_index,
    });
    app.status = "LLM: Rig workflow -> OpenAI-compatible provider -> waiting for model".to_string();

    Ok(())
}

async fn finish_pending_response(
    memory: &mut ShortTermMemory,
    config: &TuiConfig,
    app: &mut App,
) -> Result<()> {
    let pending_response = app
        .pending_response
        .take()
        .context("no pending model response to finish")?;
    let PendingResponse {
        handle,
        started_at,
        action_index,
    } = pending_response;
    app.status = "memory: saving assistant response to Valkey".to_string();

    let response = match handle
        .await
        .context("model response task failed")?
        .context("failed to run agent chat")
    {
        Ok(response) => response,
        Err(error) => {
            set_agent_action(
                app,
                action_index,
                format!(
                    "LLM: model request failed | {} elapsed",
                    format_elapsed_time(started_at.elapsed())
                ),
            );
            return Err(error);
        }
    };

    set_agent_action(
        app,
        action_index,
        format!(
            "LLM: Rig workflow -> OpenAI-compatible provider -> received model response | {} elapsed | tokens: {}",
            format_elapsed_time(started_at.elapsed()),
            token_usage_label(response.usage)
        ),
    );
    app.token_usage.add(response.usage);

    let assistant_entry = ChatEntry::for_session(
        &config.user_id,
        &config.session,
        ChatRole::Assistant,
        &response.text,
    );

    memory
        .append_chat_entry(
            &config.session,
            assistant_entry.clone(),
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save assistant chat history")?;

    show_local_entry(app, assistant_entry, config.history_limit);
    app.status = "ready".to_string();

    Ok(())
}
