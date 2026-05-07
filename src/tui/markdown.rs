use pulldown_cmark::{Event as MarkdownEvent, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub(super) fn markdown_lines(markdown: &str) -> Vec<Line<'static>> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
