use std::{
    io,
    time::{Duration, Instant},
};

use crate::llm::LlmClient;
use agent_memory::{ChatEntry, ChatRole, ShortTermMemory};
use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
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
    pub history_limit: usize,
    pub ttl_seconds: u64,
    pub preamble: &'static str,
}

struct App {
    entries: Vec<ChatEntry>,
    input: String,
    status: String,
    conversation_scroll: usize,
    pending_response: Option<PendingResponse>,
}

struct PendingResponse {
    handle: JoinHandle<Result<String>>,
    started_at: Instant,
}

impl App {
    fn new(entries: Vec<ChatEntry>) -> Self {
        Self {
            entries,
            input: String::new(),
            status: "ready".to_string(),
            conversation_scroll: 0,
            pending_response: None,
        }
    }
}

pub async fn run(memory: &mut ShortTermMemory, config: TuiConfig) -> Result<()> {
    let llm = LlmClient::from_env(config.model.clone(), config.preamble);
    let history = memory
        .chat_history(&config.session)
        .context("failed to read chat history")?;
    let mut app = App::new(history);
    let mut terminal = ratatui::init();
    if let Err(error) =
        execute!(io::stdout(), EnableMouseCapture).context("failed to enable mouse capture")
    {
        ratatui::restore();
        return Err(error);
    }

    let result = run_app(&mut terminal, memory, &llm, &config, &mut app).await;
    let mouse_result =
        execute!(io::stdout(), DisableMouseCapture).context("failed to disable mouse capture");
    ratatui::restore();

    mouse_result?;
    result
}

async fn run_app(
    terminal: &mut DefaultTerminal,
    memory: &mut ShortTermMemory,
    llm: &LlmClient,
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
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
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

                        if let Err(error) =
                            start_prompt_submission(memory, llm, config, app, history, user_entry)
                        {
                            app.status = format_error_chain(&error);
                        }
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => scroll_conversation_up(app, 3),
                MouseEventKind::ScrollDown => scroll_conversation_down(app, 3),
                _ => {}
            },
            _ => {}
        }
    }

    if let Some(pending_response) = app.pending_response.take() {
        pending_response.handle.abort();
    }

    Ok(())
}

fn start_prompt_submission(
    memory: &mut ShortTermMemory,
    llm: &LlmClient,
    config: &TuiConfig,
    app: &mut App,
    history: Vec<ChatEntry>,
    user_entry: ChatEntry,
) -> Result<()> {
    let prompt = user_entry.content.clone();

    memory
        .append_chat_entry(
            &config.session,
            user_entry,
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save user chat history")?;

    let llm = llm.clone();
    let handle = tokio::spawn(async move { llm.chat(&history, &prompt).await });

    app.pending_response = Some(PendingResponse {
        handle,
        started_at: Instant::now(),
    });
    app.status = "LLM: Rig workflow -> OpenAI-compatible request -> waiting for model".to_string();

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
    app.status = "memory: saving assistant response to Valkey".to_string();

    let response = pending_response
        .handle
        .await
        .context("model response task failed")?
        .context("failed to run agent chat")?;
    let assistant_entry = ChatEntry::for_session(
        &config.user_id,
        &config.session,
        ChatRole::Assistant,
        response,
    );

    memory
        .append_chat_entry(
            &config.session,
            assistant_entry,
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save assistant chat history")?;

    memory
        .chat_history(&config.session)
        .context("failed to read updated chat history")
        .map(|entries| {
            app.entries = entries;
            scroll_conversation_to_bottom(app);
            app.status = "ready".to_string();
        })
}

fn render(frame: &mut Frame, app: &App, config: &TuiConfig) {
    let [conversation_area, input_area, status_area] = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    render_conversation(
        frame,
        conversation_area,
        &app.entries,
        app.conversation_scroll,
    );
    render_input(frame, input_area, &app.input);
    render_status(frame, status_area, app, config);
}

fn render_conversation(
    frame: &mut Frame,
    area: Rect,
    entries: &[ChatEntry],
    scroll_from_bottom: usize,
) {
    let lines = conversation_lines(entries);
    let visible_rows = area.height.saturating_sub(2) as usize;
    let scroll = conversation_scroll(lines.len(), visible_rows, scroll_from_bottom);
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
        "Rust | Rig workflow + OpenAI SDK | session: {} | model: {} | {}",
        config.session,
        config.model,
        status_text(app)
    );
    let status_box = Paragraph::new(status)
        .block(Block::bordered().title("Status"))
        .wrap(Wrap { trim: true });

    frame.render_widget(status_box, area);
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

fn conversation_lines(entries: &[ChatEntry]) -> Vec<Line<'static>> {
    if entries.is_empty() {
        return vec![Line::from("No chat history yet.")];
    }

    entries
        .iter()
        .flat_map(|entry| {
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
        })
        .collect()
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
    trim_visible_entries(&mut app.entries, max_entries);
    scroll_conversation_to_bottom(app);
}

fn trim_visible_entries(entries: &mut Vec<ChatEntry>, max_entries: usize) {
    let extra_entries = entries.len().saturating_sub(max_entries);
    if extra_entries > 0 {
        entries.drain(0..extra_entries);
    }
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
    fn error_chain_includes_causes() {
        let error = anyhow::anyhow!("root").context("outer");

        assert_eq!(format_error_chain(&error), "error: outer | root");
    }

    #[test]
    fn show_local_entry_adds_user_message_before_response() {
        let mut app = App::new(Vec::new());
        let entry = ChatEntry::with_metadata("soonmo", "default", 1, ChatRole::User, "hello");

        show_local_entry(&mut app, entry.clone(), 20);

        assert_eq!(app.entries, vec![entry]);
    }

    #[test]
    fn show_local_entry_respects_history_limit() {
        let mut app = App::new(vec![
            ChatEntry::new(ChatRole::User, "one"),
            ChatEntry::new(ChatRole::Assistant, "two"),
        ]);

        show_local_entry(&mut app, ChatEntry::new(ChatRole::User, "three"), 2);

        assert_eq!(
            app.entries,
            vec![
                ChatEntry::new(ChatRole::Assistant, "two"),
                ChatEntry::new(ChatRole::User, "three"),
            ]
        );
    }
}
