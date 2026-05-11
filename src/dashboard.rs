use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

use agent_memory::{ChatConversation, ChatSessionSummary, DailyTokenUsage, ShortTermMemory};
use anyhow::{Context, Result, bail};

mod client;
mod grouping;

use client::dashboard_script;
use grouping::{render_cli_daily_usage, render_daily_usage_section, render_messages_by_day};

pub(crate) struct DashboardServerConfig {
    pub(crate) valkey_url: String,
    pub(crate) namespace: String,
    pub(crate) host: String,
    pub(crate) port: u16,
}

pub(crate) fn run(config: DashboardServerConfig) -> Result<()> {
    ShortTermMemory::connect(&config.valkey_url, &config.namespace)
        .with_context(|| format!("failed to connect to Valkey at {}", config.valkey_url))?;

    let address = format!("{}:{}", config.host, config.port);
    let listener =
        TcpListener::bind(&address).with_context(|| format!("failed to bind {address}"))?;

    println!("Valkey web dashboard listening on http://{address}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle_connection(&mut stream, &config) {
                    eprintln!("dashboard request failed: {error:#}");
                }
            }
            Err(error) => eprintln!("dashboard connection failed: {error}"),
        }
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream, config: &DashboardServerConfig) -> Result<()> {
    let mut buffer = [0_u8; 8192];
    let bytes_read = stream
        .read(&mut buffer)
        .context("failed to read HTTP request")?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let (method, path) = request_line_parts(&request)?;

    if method != "GET" {
        return write_response(
            stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "method not allowed",
        );
    }

    match path {
        "/" | "/dashboard" => {
            let mut memory = ShortTermMemory::connect(&config.valkey_url, &config.namespace)
                .with_context(|| format!("failed to connect to Valkey at {}", config.valkey_url))?;
            let conversations = memory
                .chat_conversations()
                .context("failed to read Valkey chat conversations")?;
            let daily_usage = memory
                .daily_token_usage()
                .context("failed to read Valkey token usage")?;
            write_response(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                &render_dashboard(&conversations, &daily_usage),
            )
        }
        "/health" => write_response(stream, "200 OK", "text/plain; charset=utf-8", "ok"),
        "/favicon.ico" => write_response(stream, "204 No Content", "text/plain", ""),
        _ => write_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "not found",
        ),
    }
}

fn request_line_parts(request: &str) -> Result<(&str, &str)> {
    let request_line = request.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let raw_path = parts.next().unwrap_or("/");

    if method.is_empty() {
        bail!("missing HTTP method");
    }

    Ok((method, raw_path.split('?').next().unwrap_or("/")))
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .context("failed to write HTTP response")
}

fn render_dashboard(conversations: &[ChatConversation], daily_usage: &[DailyTokenUsage]) -> String {
    let session_count = conversations.len();
    let message_count = conversations
        .iter()
        .map(|conversation| conversation.entries.len())
        .sum::<usize>();
    let usage_summary = render_daily_usage_section(daily_usage);
    let grouping_controls = render_grouping_controls();
    let script = dashboard_script();
    let sidebar = if conversations.is_empty() {
        r#"<div class="empty">No saved conversations yet.</div>"#.to_string()
    } else {
        conversations
            .iter()
            .map(render_sidebar_item)
            .collect::<String>()
    };
    let sessions = if conversations.is_empty() {
        r#"<div class="empty">No Valkey chat sessions found for this namespace.</div>"#.to_string()
    } else {
        conversations
            .iter()
            .map(render_conversation)
            .collect::<String>()
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Valkey Chat Dashboard</title>
<style>
:root {{ color-scheme: light; --ink: #1e2329; --muted: #69717d; --line: #d8dde4; --panel: #f6f8fa; --accent: #c43c24; --blue: #1f6feb; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; font: 14px/1.5 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: var(--ink); background: #ffffff; }}
header {{ position: sticky; top: 0; z-index: 3; display: flex; align-items: center; gap: 18px; padding: 14px 20px; border-bottom: 1px solid var(--line); background: rgba(255,255,255,.96); backdrop-filter: blur(8px); }}
h1 {{ margin: 0; font-size: 18px; letter-spacing: 0; }}
.counts {{ color: var(--muted); white-space: nowrap; }}
.filter {{ margin-left: auto; width: min(440px, 42vw); padding: 8px 10px; border: 1px solid var(--line); border-radius: 6px; font: inherit; }}
.usage {{ padding: 16px 20px; border-bottom: 1px solid var(--line); background: #fff; }}
.usage h2 {{ margin: 0 0 8px; font-size: 15px; letter-spacing: 0; }}
.usage-day {{ border: 1px solid var(--line); border-radius: 8px; margin: 8px 0; overflow: hidden; }}
.usage-day summary {{ display: flex; justify-content: space-between; gap: 12px; cursor: pointer; padding: 10px 12px; background: var(--panel); font-weight: 650; }}
.usage table {{ width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }}
.usage th, .usage td {{ padding: 7px 8px; border-bottom: 1px solid #eef1f4; text-align: right; }}
.usage th:first-child, .usage td:first-child {{ text-align: left; }}
.usage th {{ color: var(--muted); font-size: 12px; font-weight: 650; }}
.layout {{ display: grid; grid-template-columns: 300px minmax(0, 1fr); min-height: calc(100vh - 58px); }}
aside {{ position: sticky; top: 58px; align-self: start; height: calc(100vh - 58px); overflow: auto; border-right: 1px solid var(--line); background: var(--panel); padding: 12px; }}
main {{ padding: 20px; }}
.group-controls {{ display: flex; align-items: end; flex-wrap: wrap; gap: 10px; margin: 0 0 14px; padding: 12px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel); }}
.group-controls label {{ display: grid; gap: 4px; color: var(--muted); font-size: 12px; font-weight: 650; }}
.group-controls select {{ min-width: 132px; padding: 7px 8px; border: 1px solid var(--line); border-radius: 6px; background: #fff; color: var(--ink); font: inherit; }}
.group-card {{ border: 1px solid var(--line); border-radius: 8px; margin: 0 0 10px; overflow: hidden; background: #fff; }}
.group-card summary {{ cursor: pointer; padding: 10px 12px; background: var(--panel); font-weight: 650; }}
.group-card.depth-1 {{ margin-left: 10px; }}
.group-card.depth-2 {{ margin-left: 20px; }}
.group-card .message {{ padding-left: 12px; padding-right: 12px; }}
.nav-item {{ display: block; padding: 9px 10px; border-radius: 6px; color: var(--ink); text-decoration: none; border: 1px solid transparent; }}
.nav-item:hover {{ background: #fff; border-color: var(--line); }}
.nav-session {{ display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 650; }}
.nav-meta {{ color: var(--muted); font-size: 12px; }}
.empty {{ padding: 28px; border: 1px dashed var(--line); border-radius: 8px; color: var(--muted); background: var(--panel); }}
.session {{ border: 1px solid var(--line); border-radius: 8px; margin: 0 0 18px; overflow: hidden; }}
.session-header {{ padding: 14px 16px; background: var(--panel); border-bottom: 1px solid var(--line); }}
.session h2 {{ margin: 0 0 6px; font-size: 16px; letter-spacing: 0; }}
.meta {{ color: var(--muted); font-size: 13px; }}
.messages {{ padding: 12px 16px 16px; }}
.message-day {{ border: 1px solid #eef1f4; border-radius: 6px; margin: 0 0 10px; overflow: hidden; }}
.message-day summary {{ cursor: pointer; padding: 8px 10px; background: #fbfcfd; color: var(--muted); font-size: 12px; font-weight: 650; }}
.message-day .message {{ padding-left: 10px; padding-right: 10px; }}
.message {{ display: grid; grid-template-columns: 96px minmax(0, 1fr); gap: 12px; padding: 12px 0; border-bottom: 1px solid #eef1f4; }}
.message:last-child {{ border-bottom: 0; }}
.role {{ font-weight: 700; text-transform: capitalize; color: var(--accent); }}
.role.assistant {{ color: var(--blue); }}
.message-meta {{ color: var(--muted); font-size: 12px; }}
.content {{ white-space: pre-wrap; overflow-wrap: anywhere; }}
mark {{ background: #fff2a8; padding: 0 2px; }}
@media (max-width: 760px) {{
  header {{ display: block; }}
  .filter {{ width: 100%; margin: 10px 0 0; }}
  .layout {{ grid-template-columns: 1fr; }}
  aside {{ position: static; height: auto; max-height: 240px; border-right: 0; border-bottom: 1px solid var(--line); }}
  main {{ padding: 12px; }}
  .message {{ grid-template-columns: 1fr; gap: 4px; }}
}}
</style>
</head>
<body>
<header>
  <h1>Valkey Chat Dashboard</h1>
  <div class="counts">{session_count} sessions · {message_count} messages</div>
  <input id="filter" class="filter" type="search" placeholder="Filter sessions and messages" autocomplete="off">
</header>
{usage_summary}
<div class="layout">
  <aside>{sidebar}</aside>
  <main>{grouping_controls}<div id="grouped-view">{sessions}</div></main>
</div>
<script>
{script}
</script>
</body>
</html>"#
    )
}

fn render_grouping_controls() -> String {
    r#"<section class="group-controls" aria-label="Conversation grouping controls">
  <label>1st header
    <select data-group-level="1">
      <option value="date" selected>Date</option>
      <option value="session">Session</option>
      <option value="user">User</option>
    </select>
  </label>
  <label>2nd header
    <select data-group-level="2">
      <option value="date">Date</option>
      <option value="session" selected>Session</option>
      <option value="user">User</option>
    </select>
  </label>
  <label>3rd header
    <select data-group-level="3">
      <option value="date">Date</option>
      <option value="session">Session</option>
      <option value="user" selected>User</option>
    </select>
  </label>
</section>"#
        .to_string()
}

pub(crate) fn render_cli_dashboard(
    summaries: &[ChatSessionSummary],
    daily_usage: &[DailyTokenUsage],
    preview_chars: usize,
) -> String {
    let mut output = format!("Valkey chat dashboard\nsessions: {}\n", summaries.len());
    output.push_str(&render_cli_daily_usage(daily_usage));

    if summaries.is_empty() {
        output.push_str("\nnot found\n");
        return output;
    }

    for summary in summaries {
        output.push('\n');
        output.push_str(&format!("- session: {}\n", summary.session_id));
        output.push_str(&format!("  entries: {}\n", summary.entry_count));
        output.push_str(&format!(
            "  users: {}\n",
            dashboard_users(&summary.user_ids)
        ));
        output.push_str(&format!(
            "  timestamps: first={} last={}\n",
            dashboard_timestamp(summary.first_timestamp),
            dashboard_timestamp(summary.last_timestamp)
        ));
        output.push_str(&format!(
            "  last: {}: {}\n",
            summary.last_role,
            dashboard_preview(&summary.last_content, preview_chars)
        ));
    }

    output
}

fn render_sidebar_item(conversation: &ChatConversation) -> String {
    let summary = &conversation.summary;
    let id = session_anchor(&summary.session_id);

    format!(
        r##"<a class="nav-item" href="#{id}"><span class="nav-session">{}</span><span class="nav-meta">{} entries · last <time data-ts="{}">{}</time></span></a>"##,
        escape_html(&summary.session_id),
        summary.entry_count,
        summary.last_timestamp,
        escape_html(&dashboard_timestamp(summary.last_timestamp)),
    )
}

fn render_conversation(conversation: &ChatConversation) -> String {
    let summary = &conversation.summary;
    let id = session_anchor(&summary.session_id);
    let users = if summary.user_ids.is_empty() {
        "unknown".to_string()
    } else {
        escape_html(&summary.user_ids.join(", "))
    };
    let messages = render_messages_by_day(&conversation.entries);

    format!(
        r#"<section class="session" id="{id}">
<div class="session-header">
  <h2>{}</h2>
  <div class="meta">entries: {} · users: {} · first: <time data-ts="{}">{}</time> · last: <time data-ts="{}">{}</time></div>
</div>
<div class="messages">{}</div>
</section>"#,
        escape_html(&summary.session_id),
        summary.entry_count,
        users,
        summary.first_timestamp,
        escape_html(&dashboard_timestamp(summary.first_timestamp)),
        summary.last_timestamp,
        escape_html(&dashboard_timestamp(summary.last_timestamp)),
        messages,
    )
}

fn session_anchor(session_id: &str) -> String {
    let slug = session_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if slug.is_empty() {
        "session".to_string()
    } else {
        format!("session-{slug}")
    }
}

pub(crate) fn dashboard_timestamp(timestamp: u64) -> String {
    if timestamp == 0 {
        "unknown".to_string()
    } else {
        timestamp.to_string()
    }
}

fn dashboard_users(user_ids: &[String]) -> String {
    if user_ids.is_empty() {
        "unknown".to_string()
    } else {
        user_ids.join(", ")
    }
}

fn dashboard_preview(content: &str, max_chars: usize) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }

    if max_chars <= 3 {
        return collapsed.chars().take(max_chars).collect();
    }

    let mut preview = collapsed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    preview.push_str("...");
    preview
}

fn escape_html(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect::<Vec<_>>(),
            '>' => "&gt;".chars().collect::<Vec<_>>(),
            '"' => "&quot;".chars().collect::<Vec<_>>(),
            '\'' => "&#39;".chars().collect::<Vec<_>>(),
            _ => vec![character],
        })
        .collect()
}

#[cfg(test)]
mod tests;
