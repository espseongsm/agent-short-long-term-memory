use std::time::Duration;

use crate::llm::{ReasoningEffort, TokenUsage};
use agent_memory::{ChatEntry, ChatRole};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{
    interaction::apply_mouse_selection_style,
    markdown::markdown_lines,
    state::{AgentAction, App, TuiConfig},
};

#[cfg(test)]
mod tests;

pub(super) fn render(frame: &mut Frame, app: &App, config: &TuiConfig) {
    let [brand_area, conversation_area, input_area, status_area] = app_areas(frame.area());

    render_brand(frame, brand_area);
    render_conversation(frame, conversation_area, app);
    render_input(frame, input_area, &app.input);
    render_status(frame, status_area, app, config);
}

pub(super) fn app_areas(area: Rect) -> [Rect; 4] {
    Layout::vertical([
        Constraint::Length(brand_height(area.height)),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .areas(area)
}

fn brand_height(screen_height: u16) -> u16 {
    if screen_height >= 24 {
        7
    } else if screen_height >= 16 {
        3
    } else {
        0
    }
}

fn render_brand(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }

    let brand = Paragraph::new(brand_lines(area.height))
        .block(Block::bordered().title("Runtime"))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(brand, area);
}

fn brand_lines(area_height: u16) -> Vec<Line<'static>> {
    if area_height < 7 {
        return vec![Line::from(vec![
            Span::styled(
                "RUST",
                Style::new()
                    .fg(Color::Rgb(222, 92, 43))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" + ", Style::new().fg(Color::Yellow)),
            Span::styled(
                "RIG",
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " terminal agent",
                Style::new().fg(Color::Gray).add_modifier(Modifier::ITALIC),
            ),
        ])];
    }

    let rust_style = Style::new()
        .fg(Color::Rgb(222, 92, 43))
        .add_modifier(Modifier::BOLD);
    let plus_style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let rig_style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);

    vec![
        brand_banner_line(
            "RRRR   U   U   SSS  TTTTT",
            "       +       ",
            "RRRR   III   GGG ",
        ),
        brand_banner_line(
            "R   R  U   U  S       T  ",
            "      +++      ",
            "R   R   I   G    ",
        ),
        brand_banner_line(
            "RRRR   U   U   SSS    T  ",
            "     +++++     ",
            "RRRR    I   G GGG",
        ),
        brand_banner_line(
            "R  R   U   U      S   T  ",
            "      +++      ",
            "R  R    I   G   G",
        ),
        brand_banner_line(
            "R   R   UUU    SSS    T  ",
            "       +       ",
            "R   R  III   GGG ",
        ),
    ]
    .into_iter()
    .map(|mut line| {
        line.spans[0].style = rust_style;
        line.spans[1].style = plus_style;
        line.spans[2].style = rig_style;
        line
    })
    .collect()
}

fn brand_banner_line(rust: &'static str, plus: &'static str, rig: &'static str) -> Line<'static> {
    Line::from(vec![Span::raw(rust), Span::raw(plus), Span::raw(rig)])
}

fn render_conversation(frame: &mut Frame, area: Rect, app: &App) {
    let actions = visible_actions(app);
    let mut lines = conversation_lines(&app.entries, &actions);
    apply_mouse_selection_style(&mut lines, app.mouse_selection);
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
        "Rust | Rig OpenAI-compatible provider | session: {} | model: {} | reasoning: {} | tokens: {} | {}",
        config.session,
        config.model,
        reasoning_effort_label(app.reasoning_effort),
        token_usage_label(app.token_usage),
        status_text(app)
    );
    let status_box = Paragraph::new(status)
        .block(Block::bordered().title("Status"))
        .wrap(Wrap { trim: true });

    frame.render_widget(status_box, area);
}

pub(super) fn token_usage_label(usage: TokenUsage) -> String {
    if usage.has_usage() {
        format!(
            "in {} / out {} / total {}",
            usage.input_tokens, usage.output_tokens, usage.total_tokens
        )
    } else {
        "n/a".to_string()
    }
}

pub(super) fn reasoning_effort_label(reasoning_effort: Option<ReasoningEffort>) -> String {
    reasoning_effort
        .map(|reasoning_effort| reasoning_effort.to_string())
        .unwrap_or_else(|| "unset".to_string())
}

pub(super) fn status_text(app: &App) -> String {
    let Some(pending_response) = &app.pending_response else {
        return app.status.clone();
    };
    let elapsed = pending_response.started_at.elapsed();
    let spinner = pending_spinner(elapsed);

    format!(
        "LLM {spinner} | {} | {} elapsed",
        app.status,
        format_elapsed_time(elapsed)
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
        "{spinner} {} | {} elapsed",
        app.status,
        format_elapsed_time(elapsed)
    ))
}

pub(super) fn format_elapsed_time(elapsed: Duration) -> String {
    format!("{:.2}s", elapsed.as_secs_f64())
}

pub(super) fn visible_actions(app: &App) -> Vec<AgentAction> {
    let mut actions = app.actions.clone();

    if let Some(pending_response) = &app.pending_response
        && let Some(action) = actions.get_mut(pending_response.action_index)
        && let Some(content) = pending_conversation_text(app)
    {
        action.content = content;
    }

    actions
}

pub(super) fn conversation_lines(
    entries: &[ChatEntry],
    actions: &[AgentAction],
) -> Vec<Line<'static>> {
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

pub(super) fn input_tail(input: &str, max_width: usize) -> String {
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

pub(super) fn conversation_scroll(
    line_count: usize,
    visible_rows: usize,
    scroll_from_bottom: usize,
) -> usize {
    let max_scroll = line_count.saturating_sub(visible_rows);

    max_scroll.saturating_sub(scroll_from_bottom)
}

pub(super) fn scroll_conversation_up(app: &mut App, rows: usize) {
    app.conversation_scroll = app.conversation_scroll.saturating_add(rows);
}

pub(super) fn scroll_conversation_down(app: &mut App, rows: usize) {
    app.conversation_scroll = app.conversation_scroll.saturating_sub(rows);
}

pub(super) fn scroll_conversation_to_top(app: &mut App) {
    app.conversation_scroll = usize::MAX;
}

pub(super) fn scroll_conversation_to_bottom(app: &mut App) {
    app.conversation_scroll = 0;
}

pub(super) fn show_local_entry(app: &mut App, entry: ChatEntry, max_entries: usize) {
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

pub(super) fn remember_agent_action(
    app: &mut App,
    after_entry_count: usize,
    content: &str,
) -> usize {
    app.actions.push(AgentAction {
        after_entry_count,
        content: content.to_string(),
    });
    app.actions.len() - 1
}

pub(super) fn set_agent_action(app: &mut App, action_index: usize, content: String) {
    if let Some(action) = app.actions.get_mut(action_index) {
        action.content = content;
    }
}

pub(super) fn conversation_transcript(entries: &[ChatEntry], actions: &[AgentAction]) -> String {
    if entries.is_empty() && actions.is_empty() {
        return "No chat history yet.".to_string();
    }

    let mut blocks = Vec::new();
    push_action_blocks(&mut blocks, actions, 0);

    for (index, entry) in entries.iter().enumerate() {
        blocks.push(format!("{}:\n{}", entry.role, entry.content.trim_end()));
        push_action_blocks(&mut blocks, actions, index + 1);
    }

    blocks.join("\n\n")
}

fn push_action_blocks(blocks: &mut Vec<String>, actions: &[AgentAction], after_entry_count: usize) {
    blocks.extend(
        actions
            .iter()
            .filter(|action| action.after_entry_count == after_entry_count)
            .map(|action| format!("assistant action:\n{}", action.content.trim_end())),
    );
}

pub(super) fn format_error_chain(error: &anyhow::Error) -> String {
    format!(
        "error: {}",
        error
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ")
    )
}
