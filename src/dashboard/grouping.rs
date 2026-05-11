use std::collections::BTreeMap;

use agent_memory::{ChatEntry, DailyTokenUsage, utc_date};

use super::{dashboard_timestamp, escape_html, session_anchor};

pub(super) fn render_cli_daily_usage(daily_usage: &[DailyTokenUsage]) -> String {
    let mut output = "\nDaily token usage (UTC)\n".to_string();

    if daily_usage.is_empty() {
        output.push_str("not found\n");
        return output;
    }

    for usage in daily_usage {
        output.push_str(&format!(
            "- {} | calls={} input={} output={} total={} cached_input={} cache_creation={}\n",
            usage.date,
            usage.calls,
            usage.input_tokens,
            usage.output_tokens,
            usage.total_tokens,
            usage.cached_input_tokens,
            usage.cache_creation_input_tokens,
        ));

        for session in &usage.sessions {
            output.push_str(&format!(
                "  - session={} | calls={} input={} output={} total={} cached_input={} cache_creation={}\n",
                session.session_id,
                session.calls,
                session.input_tokens,
                session.output_tokens,
                session.total_tokens,
                session.cached_input_tokens,
                session.cache_creation_input_tokens,
            ));
        }
    }

    output
}

pub(super) fn render_daily_usage_section(daily_usage: &[DailyTokenUsage]) -> String {
    if daily_usage.is_empty() {
        return r#"<section class="usage"><h2>Daily token usage (UTC)</h2><div class="empty">No token usage recorded yet.</div></section>"#.to_string();
    }

    let days = daily_usage
        .iter()
        .enumerate()
        .map(|(index, usage)| {
            let open = if index == 0 { " open" } else { "" };
            let rows = usage
                .sessions
                .iter()
                .map(|session| {
                    let session_id = escape_html(&session.session_id);
                    let anchor = session_anchor(&session.session_id);
                    format!(
                        r##"<tr><td><a href="#{anchor}">{session_id}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"##,
                        session.calls,
                        session.input_tokens,
                        session.output_tokens,
                        session.total_tokens,
                        session.cached_input_tokens,
                        session.cache_creation_input_tokens,
                    )
                })
                .collect::<String>();

            format!(
                r#"<details class="usage-day"{open}><summary><span>{}</span><span>{} calls · total {} tokens</span></summary><table><thead><tr><th>Session</th><th>Calls</th><th>Input</th><th>Output</th><th>Total</th><th>Cached input</th><th>Cache creation</th></tr></thead><tbody>{rows}</tbody></table></details>"#,
                escape_html(&usage.date),
                usage.calls,
                usage.total_tokens,
            )
        })
        .collect::<String>();

    format!(r#"<section class="usage"><h2>Daily token usage (UTC)</h2>{days}</section>"#)
}

pub(super) fn render_messages_by_day(entries: &[ChatEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut groups = BTreeMap::<String, Vec<&ChatEntry>>::new();
    for entry in entries {
        groups.entry(entry_utc_date(entry)).or_default().push(entry);
    }

    groups
        .into_iter()
        .rev()
        .enumerate()
        .map(|(index, (date, entries))| {
            let open = if index == 0 { " open" } else { "" };
            let entry_count = entries.len();
            let messages = entries
                .into_iter()
                .map(render_message)
                .collect::<String>();

            format!(
                r#"<details class="message-day"{open}><summary>{} · {} messages</summary>{messages}</details>"#,
                escape_html(&date),
                entry_count,
            )
        })
        .collect()
}

fn render_message(entry: &ChatEntry) -> String {
    let role_class = entry.role.to_string();
    let date = entry_utc_date(entry);
    let user = fallback_group_value(&entry.user_id, "unknown user");
    let session = fallback_group_value(&entry.session_id, "unknown session");
    format!(
        r#"<div class="message" data-role="{}" data-ts="{}" data-date="{}" data-session="{}" data-user="{}"><div><div class="role {role_class}">{}</div><div class="message-meta"><time data-ts="{}">{}</time></div></div><div class="content">{}</div></div>"#,
        escape_html(&entry.role.to_string()),
        entry.timestamp,
        escape_html(&date),
        escape_html(session),
        escape_html(user),
        entry.role,
        entry.timestamp,
        escape_html(&dashboard_timestamp(entry.timestamp)),
        escape_html(&entry.content),
    )
}

fn entry_utc_date(entry: &ChatEntry) -> String {
    if entry.timestamp == 0 {
        "unknown date".to_string()
    } else {
        utc_date(entry.timestamp)
    }
}

fn fallback_group_value<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}
