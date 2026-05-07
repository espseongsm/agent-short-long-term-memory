use anyhow::{Context, Result};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
};
use unicode_width::UnicodeWidthChar;

use super::{
    render::{
        app_areas, conversation_lines, conversation_scroll, conversation_transcript,
        reasoning_effort_label, scroll_conversation_down, scroll_conversation_up, status_text,
        token_usage_label, visible_actions,
    },
    state::{App, MouseSelection, SelectionPoint, TuiConfig},
};

pub(super) fn handle_mouse_event(
    app: &mut App,
    terminal_area: Rect,
    mouse: MouseEvent,
) -> Result<()> {
    let [_, conversation_area, _, _] = app_areas(terminal_area);

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.mouse_selection = None;
            scroll_conversation_up(app, 3);
        }
        MouseEventKind::ScrollDown => {
            app.mouse_selection = None;
            scroll_conversation_down(app, 3);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            app.mouse_selection =
                selection_point_for_mouse(conversation_area, app, mouse.column, mouse.row).map(
                    |point| MouseSelection {
                        start: point,
                        end: point,
                        dragged: false,
                    },
                );
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let point = selection_point_for_mouse(conversation_area, app, mouse.column, mouse.row);

            if let (Some(selection), Some(point)) = (app.mouse_selection.as_mut(), point) {
                selection.end = point;
                selection.dragged = true;
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let Some(mut selection) = app.mouse_selection.take() else {
                return Ok(());
            };

            if let Some(point) =
                selection_point_for_mouse(conversation_area, app, mouse.column, mouse.row)
            {
                selection.end = point;
            }

            if !selection.is_active() {
                return Ok(());
            }

            let actions = visible_actions(app);
            let lines = conversation_lines(&app.entries, &actions);

            if let Some(text) = selected_conversation_text(&lines, selection) {
                copy_text_to_clipboard(&text).context("failed to copy mouse selection")?;
                app.status = "clipboard: copied mouse selection".to_string();
            }
        }
        _ => {}
    }

    Ok(())
}

fn selection_point_for_mouse(
    conversation_area: Rect,
    app: &App,
    column: u16,
    row: u16,
) -> Option<SelectionPoint> {
    let content_area = conversation_content_area(conversation_area);

    if content_area.width == 0
        || content_area.height == 0
        || column < content_area.x
        || column >= content_area.right()
        || row < content_area.y
        || row >= content_area.bottom()
    {
        return None;
    }

    let actions = visible_actions(app);
    let line_count = conversation_lines(&app.entries, &actions).len();
    let visible_rows = content_area.height as usize;
    let scroll = conversation_scroll(line_count, visible_rows, app.conversation_scroll);
    let line = scroll + usize::from(row - content_area.y);

    if line >= line_count {
        return None;
    }

    Some(SelectionPoint {
        line,
        column: usize::from(column - content_area.x),
    })
}

fn conversation_content_area(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

pub(super) fn apply_mouse_selection_style(
    lines: &mut [Line<'static>],
    selection: Option<MouseSelection>,
) {
    let Some(selection) = selection.filter(|selection| selection.is_active()) else {
        return;
    };
    let (start, end) = selection.ordered_points();

    for (line_index, line) in lines.iter_mut().enumerate() {
        if line_index >= start.line && line_index <= end.line {
            *line = std::mem::take(line).patch_style(Style::new().bg(Color::DarkGray));
        }
    }
}

fn selected_conversation_text(lines: &[Line<'_>], selection: MouseSelection) -> Option<String> {
    if !selection.is_active() {
        return None;
    }

    let (start, end) = selection.ordered_points();
    let mut selected_lines = Vec::new();

    for line_index in start.line..=end.line {
        let Some(line) = lines.get(line_index) else {
            break;
        };
        let text = line_plain_text(line);
        let selected_text = if start.line == end.line {
            text_by_display_columns(&text, start.column, end.column)
        } else if line_index == start.line {
            text_by_display_columns(&text, start.column, usize::MAX)
        } else if line_index == end.line {
            text_by_display_columns(&text, 0, end.column)
        } else {
            text
        };

        selected_lines.push(selected_text.trim_end().to_string());
    }

    let text = selected_lines.join("\n").trim_end().to_string();

    if text.is_empty() { None } else { Some(text) }
}

fn line_plain_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn text_by_display_columns(text: &str, start: usize, end: usize) -> String {
    if end <= start {
        return String::new();
    }

    let mut selected = String::new();
    let mut column = 0usize;

    for character in text.chars() {
        let width = character.width().unwrap_or(0);
        let next_column = column.saturating_add(width);
        let overlaps_selection = (width == 0 && column >= start && column < end)
            || (next_column > start && column < end);

        if overlaps_selection {
            selected.push(character);
        }

        column = next_column;

        if column >= end {
            break;
        }
    }

    selected
}

pub(super) fn copy_prompt_to_clipboard(input: &str) -> Result<()> {
    copy_text_to_clipboard(input).context("failed to copy prompt to clipboard")
}

pub(super) fn copy_conversation_to_clipboard(app: &App, config: &TuiConfig) -> Result<()> {
    copy_text_to_clipboard(&conversation_clipboard_text(app, config))
        .context("failed to copy conversation to clipboard")
}

fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("failed to open clipboard")?;
    clipboard
        .set_text(text.to_string())
        .context("clipboard write failed")
}

fn conversation_clipboard_text(app: &App, config: &TuiConfig) -> String {
    let actions = visible_actions(app);

    format!(
        "Session: {}\nModel: {}\nReasoning: {}\nToken usage: {}\nStatus: {}\n\nConversation:\n{}",
        config.session,
        config.model,
        reasoning_effort_label(app.reasoning_effort),
        token_usage_label(app.token_usage),
        status_text(app),
        conversation_transcript(&app.entries, &actions)
    )
}

pub(super) fn paste_text_from_clipboard() -> Result<String> {
    let mut clipboard = arboard::Clipboard::new().context("failed to open clipboard")?;
    clipboard
        .get_text()
        .context("failed to read text from clipboard")
}

pub(super) fn push_pasted_text(input: &mut String, text: &str) {
    input.push_str(&text.replace("\r\n", " ").replace(['\r', '\n'], " "));
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_memory::{ChatEntry, ChatRole};

    #[test]
    fn selected_conversation_text_copies_dragged_rows() {
        let lines = vec![Line::from("user: hello"), Line::from("assistant: world")];
        let selection = MouseSelection {
            start: SelectionPoint { line: 0, column: 6 },
            end: SelectionPoint {
                line: 1,
                column: 16,
            },
            dragged: true,
        };

        assert_eq!(
            selected_conversation_text(&lines, selection),
            Some("hello\nassistant: world".to_string())
        );
    }

    #[test]
    fn selected_conversation_text_respects_korean_display_width() {
        let lines = vec![Line::from("abc안녕")];
        let selection = MouseSelection {
            start: SelectionPoint { line: 0, column: 3 },
            end: SelectionPoint { line: 0, column: 7 },
            dragged: true,
        };

        assert_eq!(
            selected_conversation_text(&lines, selection),
            Some("안녕".to_string())
        );
    }

    #[test]
    fn selection_point_for_mouse_maps_conversation_content_cells() {
        let app = App::new(vec![ChatEntry::new(ChatRole::User, "hello")], None);
        let conversation_area = Rect::new(0, 0, 80, 8);

        assert_eq!(
            selection_point_for_mouse(conversation_area, &app, 1, 1),
            Some(SelectionPoint { line: 0, column: 0 })
        );
        assert_eq!(
            selection_point_for_mouse(conversation_area, &app, 0, 1),
            None
        );
    }

    #[test]
    fn push_pasted_text_keeps_prompt_single_line() {
        let mut input = "hello ".to_string();

        push_pasted_text(&mut input, "one\r\ntwo\rthree\nfour");

        assert_eq!(input, "hello one two three four");
    }
}
