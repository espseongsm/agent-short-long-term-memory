use std::time::Duration;

use crate::llm::LlmClient;
use agent_memory::{ChatEntry, ChatRole, ShortTermMemory};
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

pub struct TuiConfig {
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
}

impl App {
    fn new(entries: Vec<ChatEntry>) -> Self {
        Self {
            entries,
            input: String::new(),
            status: "ready".to_string(),
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

    let result = run_app(&mut terminal, memory, &llm, &config, &mut app).await;
    ratatui::restore();

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
        terminal.draw(|frame| render(frame, app, config))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Esc => break,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Char(character) => app.input.push(character),
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Enter => {
                let prompt = app.input.trim().to_string();
                if prompt.is_empty() {
                    continue;
                }

                app.input.clear();
                app.status = "thinking".to_string();
                terminal.draw(|frame| render(frame, app, config))?;

                match submit_prompt(memory, llm, config, prompt).await {
                    Ok(entries) => {
                        app.entries = entries;
                        app.status = "ready".to_string();
                    }
                    Err(error) => {
                        app.status = format_error_chain(&error);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

async fn submit_prompt(
    memory: &mut ShortTermMemory,
    llm: &LlmClient,
    config: &TuiConfig,
    prompt: String,
) -> Result<Vec<ChatEntry>> {
    let history = memory
        .chat_history(&config.session)
        .context("failed to read chat history")?;
    let response = llm
        .chat(&history, prompt.as_str())
        .await
        .context("failed to run agent chat")?;

    memory
        .append_chat_entry(
            &config.session,
            ChatEntry::new(ChatRole::User, prompt),
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save user chat history")?;
    memory
        .append_chat_entry(
            &config.session,
            ChatEntry::new(ChatRole::Assistant, response),
            config.history_limit,
            config.ttl_seconds,
        )
        .context("failed to save assistant chat history")?;

    memory
        .chat_history(&config.session)
        .context("failed to read updated chat history")
}

fn render(frame: &mut Frame, app: &App, config: &TuiConfig) {
    let [logo_area, conversation_area, input_area, status_area] = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    render_logo(frame, logo_area);

    render_conversation(frame, conversation_area, &app.entries);
    render_input(frame, input_area, &app.input);
    render_status(frame, status_area, app, config);
}

fn render_logo(frame: &mut Frame, area: Rect) {
    let logo = "\
RRRR   U   U   SSS   TTTTT
R   R  U   U  S        T
RRRR   U   U   SSS     T
R  R   U   U      S    T
R   R   UUU    SSS     T";
    let logo_para = Paragraph::new(logo)
        .alignment(ratatui::layout::Alignment::Center)
        .style(
            Style::new()
                .fg(Color::LightRed)
                .add_modifier(ratatui::style::Modifier::BOLD),
        );
    frame.render_widget(logo_para, area);
}

fn render_conversation(frame: &mut Frame, area: Rect, entries: &[ChatEntry]) {
    let lines = conversation_lines(entries);
    let visible_rows = area.height.saturating_sub(2) as usize;
    let scroll = lines.len().saturating_sub(visible_rows) as u16;
    let conversation = Paragraph::new(lines)
        .block(Block::bordered().title("Conversation"))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(conversation, area);
}

fn render_input(frame: &mut Frame, area: Rect, input: &str) {
    let visible_width = area.width.saturating_sub(2) as usize;
    let visible_input = input_tail(input, visible_width);
    let input_box = Paragraph::new(visible_input.clone()).block(Block::bordered().title("Prompt"));
    frame.render_widget(input_box, area);

    let cursor_x = area.x + 1 + visible_input.chars().count() as u16;
    let cursor_x = cursor_x.min(area.x + area.width.saturating_sub(2));
    frame.set_cursor_position((cursor_x, area.y + 1));
}

fn render_status(frame: &mut Frame, area: Rect, app: &App, config: &TuiConfig) {
    let status = format!(
        "Rust | Rig workflow + OpenAI SDK | session: {} | model: {} | {}",
        config.session, config.model, app.status
    );
    let status_box = Paragraph::new(status)
        .block(Block::bordered().title("Status"))
        .wrap(Wrap { trim: true });

    frame.render_widget(status_box, area);
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
            let mut lines = entry.content.lines();
            let first = lines.next().unwrap_or_default();
            let mut rendered = vec![Line::from(vec![
                Span::styled(format!("{}: ", entry.role), Style::new().fg(color)),
                Span::raw(first.to_string()),
            ])];

            rendered.extend(lines.map(|line| Line::from(format!("  {line}"))));
            rendered
        })
        .collect()
}

fn input_tail(input: &str, max_chars: usize) -> String {
    let input_len = input.chars().count();
    input
        .chars()
        .skip(input_len.saturating_sub(max_chars))
        .collect()
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
    fn error_chain_includes_causes() {
        let error = anyhow::anyhow!("root").context("outer");

        assert_eq!(format_error_chain(&error), "error: outer | root");
    }
}
