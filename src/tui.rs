use std::{
    io,
    time::{Duration, Instant},
};

use crate::{
    agent_workflow,
    llm::{LlmClient, ReasoningEffort},
    long_term_memory,
    weather::WeatherClient,
    web_search::{DEFAULT_WEB_SEARCH_LIMIT, WebSearchClient},
};
use agent_memory::{ChatEntry, ChatRole, ShortTermMemory};
use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
    },
    execute,
};
use pulldown_cmark::{Event as MarkdownEvent, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};
use tokio::task::JoinHandle;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub struct TuiConfig {
    pub user_id: String,
    pub session: String,
    pub model: String,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub history_limit: usize,
    pub ttl_seconds: u64,
    pub preamble: &'static str,
    pub amplifier_preamble: &'static str,
    pub pgvector_url: Option<String>,
}

struct App {
    entries: Vec<ChatEntry>,
    actions: Vec<AgentAction>,
    input: String,
    status: String,
    reasoning_effort: Option<ReasoningEffort>,
    conversation_scroll: usize,
    pending_response: Option<PendingResponse>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentAction {
    after_entry_count: usize,
    content: String,
}

struct PendingResponse {
    handle: JoinHandle<Result<String>>,
    started_at: Instant,
    action_index: usize,
}

#[derive(Clone, Copy)]
struct AgentServices<'a> {
    weather: &'a WeatherClient,
    web_search: &'a WebSearchClient,
}

impl App {
    fn new(entries: Vec<ChatEntry>, reasoning_effort: Option<ReasoningEffort>) -> Self {
        Self {
            entries,
            actions: Vec::new(),
            input: String::new(),
            status: "ready".to_string(),
            reasoning_effort,
            conversation_scroll: 0,
            pending_response: None,
        }
    }
}

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

                        start_prompt_submission(config, app, history, prompt_for_llm);
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => scroll_conversation_up(app, 3),
                MouseEventKind::ScrollDown => scroll_conversation_down(app, 3),
                _ => {}
            },
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

async fn submit_web_search(
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
        "web search: querying DuckDuckGo-compatible endpoint",
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

async fn submit_request_amplification(
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
        config.amplifier_preamble,
        app.reasoning_effort,
    );
    let amplified = match request_amplifier.chat(&[], request).await {
        Ok(amplified) => amplified,
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

fn start_prompt_submission(
    config: &TuiConfig,
    app: &mut App,
    history: Vec<ChatEntry>,
    prompt: String,
) {
    let llm = LlmClient::from_env(config.model.clone(), config.preamble, app.reasoning_effort);
    let action_index = remember_agent_action(
        app,
        app.entries.len(),
        "LLM: Rig workflow -> OpenAI-compatible request -> waiting for model",
    );
    let handle = tokio::spawn(async move { llm.chat(&history, &prompt).await });

    app.pending_response = Some(PendingResponse {
        handle,
        started_at: Instant::now(),
        action_index,
    });
    app.status = "LLM: Rig workflow -> OpenAI-compatible request -> waiting for model".to_string();
}

async fn prepare_automatic_chat_prompt(
    terminal: &mut DefaultTerminal,
    services: AgentServices<'_>,
    config: &TuiConfig,
    app: &mut App,
    history: &[ChatEntry],
    prompt: &str,
) -> Result<String> {
    let actions = agent_workflow::automatic_actions(prompt);
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
            config.amplifier_preamble,
            app.reasoning_effort,
        );
        match request_amplifier.chat(history, prompt).await {
            Ok(amplified) => {
                set_agent_action(
                    app,
                    action_index,
                    "request amplifier: produced automatic amplified request".to_string(),
                );
                prompt_for_llm = agent_workflow::prompt_with_amplification(prompt, &amplified);
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
        }

        terminal.draw(|frame| render(frame, app, config))?;
    }

    if actions.weather {
        let location = agent_workflow::weather_location_query(prompt);
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
                    "weather: automatic request failed; continuing without weather context"
                        .to_string(),
                );
                app.status = format_error_chain(&error);
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

fn web_search_command_query(input: &str) -> Option<&str> {
    let input = input.trim();

    input
        .strip_prefix("/search")
        .or_else(|| input.strip_prefix("/web"))
        .map(str::trim)
}

fn amplify_command_request(input: &str) -> Option<&str> {
    input.trim().strip_prefix("/amplify").map(str::trim)
}

fn reasoning_effort_command_value(input: &str) -> Option<&str> {
    input
        .trim()
        .strip_prefix("/reasoning")
        .or_else(|| input.trim().strip_prefix("/effort"))
        .map(str::trim)
}

fn parse_reasoning_effort_command(value: &str) -> Result<Option<ReasoningEffort>> {
    let value = value.trim();

    if value.eq_ignore_ascii_case("unset") || value.eq_ignore_ascii_case("default") {
        return Ok(None);
    }

    ReasoningEffort::from_env_value(Some(value))
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
                    "LLM: model request failed | {}s elapsed",
                    started_at.elapsed().as_secs()
                ),
            );
            return Err(error);
        }
    };

    set_agent_action(
        app,
        action_index,
        format!(
            "LLM: Rig workflow -> OpenAI-compatible request -> received model response | {}s elapsed",
            started_at.elapsed().as_secs()
        ),
    );

    let assistant_entry = ChatEntry::for_session(
        &config.user_id,
        &config.session,
        ChatRole::Assistant,
        &response,
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

fn render(frame: &mut Frame, app: &App, config: &TuiConfig) {
    let [conversation_area, input_area, status_area] = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    render_conversation(frame, conversation_area, app);
    render_input(frame, input_area, &app.input);
    render_status(frame, status_area, app, config);
}

fn render_conversation(frame: &mut Frame, area: Rect, app: &App) {
    let actions = visible_actions(app);
    let lines = conversation_lines(&app.entries, &actions);
    let visible_rows = area.height.saturating_sub(2) as usize;
    let scroll = conversation_scroll(lines.len(), visible_rows, app.conversation_scroll);
    let conversation = Paragraph::new(lines)
        .block(Block::bordered().title("Conversation"))
        .wrap(Wrap { trim: false })
        .scroll((scroll.min(u16::MAX as usize) as u16, 0));

    frame.render_widget(conversation, area);
}

fn render_input(frame: &mut Frame, area: Rect, input: &str) {
    let visible_width = area.width.saturating_sub(2) as usize;
    let visible_input = input_tail(input, visible_width);
    let input_box = Paragraph::new(visible_input.clone()).block(Block::bordered().title("Prompt"));
    frame.render_widget(input_box, area);

    let cursor_x = area.x + 1 + UnicodeWidthStr::width(visible_input.as_str()) as u16;
    let cursor_x = cursor_x.min(area.x + area.width.saturating_sub(2));
    frame.set_cursor_position((cursor_x, area.y + 1));
}

fn render_status(frame: &mut Frame, area: Rect, app: &App, config: &TuiConfig) {
    let status = format!(
        "Rust | Rig workflow + OpenAI SDK | session: {} | model: {} | reasoning: {} | {}",
        config.session,
        config.model,
        reasoning_effort_label(app.reasoning_effort),
        status_text(app)
    );
    let status_box = Paragraph::new(status)
        .block(Block::bordered().title("Status"))
        .wrap(Wrap { trim: true });

    frame.render_widget(status_box, area);
}

fn reasoning_effort_label(reasoning_effort: Option<ReasoningEffort>) -> String {
    reasoning_effort
        .map(|reasoning_effort| reasoning_effort.to_string())
        .unwrap_or_else(|| "unset".to_string())
}

fn status_text(app: &App) -> String {
    let Some(pending_response) = &app.pending_response else {
        return app.status.clone();
    };
    let elapsed = pending_response.started_at.elapsed();
    let spinner = pending_spinner(elapsed);

    format!(
        "LLM {spinner} | {} | {}s elapsed",
        app.status,
        elapsed.as_secs()
    )
}

fn pending_spinner(elapsed: Duration) -> &'static str {
    match (elapsed.as_millis() / 250) % 4 {
        0 => "|",
        1 => "/",
        2 => "-",
        _ => "\\",
    }
}

fn pending_conversation_text(app: &App) -> Option<String> {
    let pending_response = app.pending_response.as_ref()?;
    let elapsed = pending_response.started_at.elapsed();
    let spinner = pending_spinner(elapsed);

    Some(format!(
        "{spinner} {} | {}s elapsed",
        app.status,
        elapsed.as_secs()
    ))
}

fn visible_actions(app: &App) -> Vec<AgentAction> {
    let mut actions = app.actions.clone();

    if let Some(pending_response) = &app.pending_response
        && let Some(action) = actions.get_mut(pending_response.action_index)
        && let Some(content) = pending_conversation_text(app)
    {
        action.content = content;
    }

    actions
}

fn conversation_lines(entries: &[ChatEntry], actions: &[AgentAction]) -> Vec<Line<'static>> {
    if entries.is_empty() && actions.is_empty() {
        return vec![Line::from("No chat history yet.")];
    }

    let mut lines = Vec::new();
    push_action_lines(&mut lines, actions, 0);

    for (index, entry) in entries.iter().enumerate() {
        lines.extend({
            let color = match entry.role {
                ChatRole::User => Color::Cyan,
                ChatRole::Assistant => Color::Green,
            };
            let role = Span::styled(
                format!("{}: ", entry.role),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            );
            let markdown_lines = markdown_lines(&entry.content);

            let mut rendered = Vec::new();

            for (index, line) in markdown_lines.into_iter().enumerate() {
                let mut spans = Vec::new();

                if index == 0 {
                    spans.push(role.clone());
                } else {
                    spans.push(Span::raw("  "));
                }

                spans.extend(line.spans);
                rendered.push(Line::from(spans));
            }

            rendered
        });
        push_action_lines(&mut lines, actions, index + 1);
    }

    lines
}

fn push_action_lines(
    lines: &mut Vec<Line<'static>>,
    actions: &[AgentAction],
    after_entry_count: usize,
) {
    lines.extend(
        actions
            .iter()
            .filter(|action| action.after_entry_count == after_entry_count)
            .map(|action| action_line(&action.content)),
    );
}

fn action_line(action: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "assistant: ",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::styled(action.to_string(), Style::new().fg(Color::Yellow)),
    ])
}

fn markdown_lines(markdown: &str) -> Vec<Line<'static>> {
    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
    );
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut strong_depth = 0usize;
    let mut emphasis_depth = 0usize;
    let mut code_block_depth = 0usize;
    let mut quote_depth = 0usize;

    for event in parser {
        match event {
            MarkdownEvent::Start(tag) => match tag {
                Tag::Paragraph => finish_non_empty_line(&mut lines, &mut current),
                Tag::Heading { level, .. } => {
                    finish_non_empty_line(&mut lines, &mut current);
                    current.push(Span::styled(heading_marker(level), markdown_style(1, 0, 0)));
                    strong_depth += 1;
                }
                Tag::BlockQuote(_) => {
                    quote_depth += 1;
                    finish_non_empty_line(&mut lines, &mut current);
                    current.push(Span::styled("> ", Style::new().fg(Color::DarkGray)));
                }
                Tag::CodeBlock(_) => {
                    finish_non_empty_line(&mut lines, &mut current);
                    code_block_depth += 1;
                }
                Tag::List(_) => finish_non_empty_line(&mut lines, &mut current),
                Tag::Item => {
                    finish_non_empty_line(&mut lines, &mut current);
                    current.push(Span::raw("- "));
                }
                Tag::Emphasis => emphasis_depth += 1,
                Tag::Strong => strong_depth += 1,
                _ => {}
            },
            MarkdownEvent::End(tag) => match tag {
                TagEnd::Paragraph
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::List(_)
                | TagEnd::Table
                | TagEnd::TableHead
                | TagEnd::TableRow
                | TagEnd::TableCell => finish_non_empty_line(&mut lines, &mut current),
                TagEnd::Heading(_) => {
                    strong_depth = strong_depth.saturating_sub(1);
                    finish_non_empty_line(&mut lines, &mut current);
                }
                TagEnd::BlockQuote(_) => {
                    quote_depth = quote_depth.saturating_sub(1);
                    finish_non_empty_line(&mut lines, &mut current);
                }
                TagEnd::Emphasis => emphasis_depth = emphasis_depth.saturating_sub(1),
                TagEnd::Strong => strong_depth = strong_depth.saturating_sub(1),
                _ => {}
            },
            MarkdownEvent::Text(text) => push_markdown_text(
                &mut lines,
                &mut current,
                text.as_ref(),
                markdown_style(strong_depth, emphasis_depth, code_block_depth),
                quote_depth,
            ),
            MarkdownEvent::Code(code) => {
                current.push(Span::styled(
                    code.to_string(),
                    Style::new().fg(Color::Yellow),
                ));
            }
            MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => {
                finish_non_empty_line(&mut lines, &mut current);
                if quote_depth > 0 {
                    current.push(Span::styled("> ", Style::new().fg(Color::DarkGray)));
                }
            }
            MarkdownEvent::Rule => {
                finish_non_empty_line(&mut lines, &mut current);
                lines.push(Line::from(Span::styled(
                    "---",
                    Style::new().fg(Color::DarkGray),
                )));
            }
            _ => {}
        }
    }

    finish_non_empty_line(&mut lines, &mut current);

    if lines.is_empty() {
        vec![Line::from("")]
    } else {
        lines
    }
}

fn push_markdown_text(
    lines: &mut Vec<Line<'static>>,
    current: &mut Vec<Span<'static>>,
    text: &str,
    style: Style,
    quote_depth: usize,
) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            finish_non_empty_line(lines, current);

            if quote_depth > 0 {
                current.push(Span::styled("> ", Style::new().fg(Color::DarkGray)));
            }
        }

        if !line.is_empty() {
            current.push(Span::styled(line.to_string(), style));
        }
    }
}

fn finish_non_empty_line(lines: &mut Vec<Line<'static>>, current: &mut Vec<Span<'static>>) {
    if !current.is_empty() {
        lines.push(Line::from(std::mem::take(current)));
    }
}

fn markdown_style(strong_depth: usize, emphasis_depth: usize, code_block_depth: usize) -> Style {
    let mut style = Style::new();

    if strong_depth > 0 {
        style = style.add_modifier(Modifier::BOLD);
    }

    if emphasis_depth > 0 {
        style = style.add_modifier(Modifier::ITALIC);
    }

    if code_block_depth > 0 {
        style = style.fg(Color::Yellow);
    }

    style
}

fn heading_marker(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "# ",
        HeadingLevel::H2 => "## ",
        HeadingLevel::H3 => "### ",
        HeadingLevel::H4 => "#### ",
        HeadingLevel::H5 => "##### ",
        HeadingLevel::H6 => "###### ",
    }
}

fn input_tail(input: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut chars = Vec::new();

    for character in input.chars().rev() {
        let character_width = character.width().unwrap_or(0);

        if width + character_width > max_width {
            break;
        }

        width += character_width;
        chars.push(character);
    }

    chars.into_iter().rev().collect()
}

fn conversation_scroll(line_count: usize, visible_rows: usize, scroll_from_bottom: usize) -> usize {
    let max_scroll = line_count.saturating_sub(visible_rows);

    max_scroll.saturating_sub(scroll_from_bottom)
}

fn scroll_conversation_up(app: &mut App, rows: usize) {
    app.conversation_scroll = app.conversation_scroll.saturating_add(rows);
}

fn scroll_conversation_down(app: &mut App, rows: usize) {
    app.conversation_scroll = app.conversation_scroll.saturating_sub(rows);
}

fn scroll_conversation_to_top(app: &mut App) {
    app.conversation_scroll = usize::MAX;
}

fn scroll_conversation_to_bottom(app: &mut App) {
    app.conversation_scroll = 0;
}

fn show_local_entry(app: &mut App, entry: ChatEntry, max_entries: usize) {
    app.entries.push(entry);
    let removed_entries = trim_visible_entries(&mut app.entries, max_entries);
    trim_actions_after_entry_removal(&mut app.actions, removed_entries);
    scroll_conversation_to_bottom(app);
}

fn trim_visible_entries(entries: &mut Vec<ChatEntry>, max_entries: usize) -> usize {
    let extra_entries = entries.len().saturating_sub(max_entries);
    if extra_entries > 0 {
        entries.drain(0..extra_entries);
    }

    extra_entries
}

fn trim_actions_after_entry_removal(actions: &mut Vec<AgentAction>, removed_entries: usize) {
    if removed_entries == 0 {
        return;
    }

    for action in actions.iter_mut() {
        action.after_entry_count = action.after_entry_count.saturating_sub(removed_entries);
    }
    actions.retain(|action| action.after_entry_count > 0);
}

fn remember_agent_action(app: &mut App, after_entry_count: usize, content: &str) -> usize {
    app.actions.push(AgentAction {
        after_entry_count,
        content: content.to_string(),
    });
    app.actions.len() - 1
}

fn set_agent_action(app: &mut App, action_index: usize, content: String) {
    if let Some(action) = app.actions.get_mut(action_index) {
        action.content = content;
    }
}

fn copy_prompt_to_clipboard(input: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("failed to open clipboard")?;
    clipboard
        .set_text(input.to_string())
        .context("failed to copy prompt to clipboard")
}

fn paste_text_from_clipboard() -> Result<String> {
    let mut clipboard = arboard::Clipboard::new().context("failed to open clipboard")?;
    clipboard
        .get_text()
        .context("failed to read text from clipboard")
}

fn push_pasted_text(input: &mut String, text: &str) {
    input.push_str(&text.replace("\r\n", "\n").replace('\r', "\n"));
}

fn format_error_chain(error: &anyhow::Error) -> String {
    format!(
        "error: {}",
        error
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_tail_keeps_short_input() {
        assert_eq!(input_tail("hello", 10), "hello");
    }

    #[test]
    fn input_tail_trims_from_the_left() {
        assert_eq!(input_tail("hello", 3), "llo");
    }

    #[test]
    fn input_tail_respects_korean_display_width() {
        assert_eq!(input_tail("abc안녕", 4), "안녕");
    }

    #[test]
    fn conversation_scroll_defaults_to_bottom() {
        assert_eq!(conversation_scroll(20, 5, 0), 15);
    }

    #[test]
    fn conversation_scroll_can_move_up_from_bottom() {
        assert_eq!(conversation_scroll(20, 5, 3), 12);
    }

    #[test]
    fn conversation_scroll_clamps_to_top() {
        assert_eq!(conversation_scroll(20, 5, usize::MAX), 0);
    }

    #[test]
    fn pending_spinner_cycles() {
        assert_eq!(pending_spinner(Duration::from_millis(0)), "|");
        assert_eq!(pending_spinner(Duration::from_millis(250)), "/");
        assert_eq!(pending_spinner(Duration::from_millis(500)), "-");
        assert_eq!(pending_spinner(Duration::from_millis(750)), "\\");
    }

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
    fn parses_reasoning_effort_command_values() {
        assert_eq!(
            parse_reasoning_effort_command("low").unwrap(),
            Some(ReasoningEffort::Low)
        );
        assert_eq!(parse_reasoning_effort_command("unset").unwrap(), None);
        assert!(parse_reasoning_effort_command("fast").is_err());
    }

    #[test]
    fn markdown_lines_renders_markdown_blocks() {
        let lines = markdown_lines("# Title\n\n- one\n- **two**");

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "# ");
        assert_eq!(lines[0].spans[1].content, "Title");
        assert_eq!(lines[1].spans[0].content, "- ");
        assert_eq!(lines[1].spans[1].content, "one");
        assert_eq!(lines[2].spans[0].content, "- ");
        assert_eq!(lines[2].spans[1].content, "two");
    }

    #[test]
    fn conversation_lines_keep_agent_action_between_user_and_assistant() {
        let entries = vec![
            ChatEntry::new(ChatRole::User, "hello"),
            ChatEntry::new(ChatRole::Assistant, "hi"),
        ];
        let actions = vec![AgentAction {
            after_entry_count: 1,
            content: "LLM: received model response".to_string(),
        }];

        let lines = conversation_lines(&entries, &actions);

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "user: ");
        assert_eq!(lines[1].spans[0].content, "assistant: ");
        assert_eq!(lines[1].spans[1].content, "LLM: received model response");
        assert_eq!(lines[2].spans[0].content, "assistant: ");
    }

    #[test]
    fn error_chain_includes_causes() {
        let error = anyhow::anyhow!("root").context("outer");

        assert_eq!(format_error_chain(&error), "error: outer | root");
    }

    #[test]
    fn push_pasted_text_normalizes_line_endings() {
        let mut input = "hello ".to_string();

        push_pasted_text(&mut input, "one\r\ntwo\rthree");

        assert_eq!(input, "hello one\ntwo\nthree");
    }

    #[test]
    fn show_local_entry_adds_user_message_before_response() {
        let mut app = App::new(Vec::new(), None);
        let entry = ChatEntry::with_metadata("soonmo", "default", 1, ChatRole::User, "hello");

        show_local_entry(&mut app, entry.clone(), 20);

        assert_eq!(app.entries, vec![entry]);
    }

    #[test]
    fn show_local_entry_respects_history_limit() {
        let mut app = App::new(
            vec![
                ChatEntry::new(ChatRole::User, "one"),
                ChatEntry::new(ChatRole::Assistant, "two"),
            ],
            None,
        );

        show_local_entry(&mut app, ChatEntry::new(ChatRole::User, "three"), 2);

        assert_eq!(
            app.entries,
            vec![
                ChatEntry::new(ChatRole::Assistant, "two"),
                ChatEntry::new(ChatRole::User, "three"),
            ]
        );
    }

    #[test]
    fn show_local_entry_drops_actions_for_trimmed_entries() {
        let mut app = App::new(
            vec![
                ChatEntry::new(ChatRole::User, "one"),
                ChatEntry::new(ChatRole::Assistant, "two"),
            ],
            None,
        );
        remember_agent_action(&mut app, 1, "LLM: old action");

        show_local_entry(&mut app, ChatEntry::new(ChatRole::User, "three"), 2);

        assert!(app.actions.is_empty());
    }

    #[test]
    fn show_local_entry_shifts_actions_after_trimmed_entries() {
        let mut app = App::new(
            vec![
                ChatEntry::new(ChatRole::User, "one"),
                ChatEntry::new(ChatRole::Assistant, "two"),
            ],
            None,
        );
        remember_agent_action(&mut app, 2, "LLM: kept action");

        show_local_entry(&mut app, ChatEntry::new(ChatRole::User, "three"), 2);

        assert_eq!(
            app.actions,
            vec![AgentAction {
                after_entry_count: 1,
                content: "LLM: kept action".to_string(),
            }]
        );
    }
}
