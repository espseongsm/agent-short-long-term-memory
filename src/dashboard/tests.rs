use super::*;
use agent_memory::{ChatEntry, ChatRole, ChatSessionSummary, SessionTokenUsage};

#[test]
fn escapes_html_content() {
    assert_eq!(
        escape_html("<script>alert('x')</script>"),
        "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"
    );
}

#[test]
fn renders_all_conversation_messages() {
    let conversations = vec![ChatConversation {
        summary: ChatSessionSummary {
            session_id: "work".to_string(),
            entry_count: 2,
            user_ids: vec!["soonmo".to_string()],
            first_timestamp: 10,
            last_timestamp: 20,
            last_role: ChatRole::Assistant,
            last_content: "done".to_string(),
        },
        entries: vec![
            ChatEntry::with_metadata("soonmo", "work", 10, ChatRole::User, "hello"),
            ChatEntry::with_metadata("soonmo", "work", 20, ChatRole::Assistant, "done"),
        ],
    }];

    let html = render_dashboard(&conversations, &[]);

    assert!(html.contains("data-group-level=\"1\""));
    assert!(html.contains("Date</option>"));
    assert!(html.contains("work"));
    assert!(html.contains("hello"));
    assert!(html.contains("done"));
    assert!(!html.contains("2 sessions"));
}

#[test]
fn renders_daily_token_usage_table() {
    let usage = vec![DailyTokenUsage {
        date: "2026-05-11".to_string(),
        calls: 2,
        input_tokens: 30,
        output_tokens: 20,
        total_tokens: 50,
        cached_input_tokens: 3,
        cache_creation_input_tokens: 4,
        sessions: vec![SessionTokenUsage {
            session_id: "work".to_string(),
            calls: 2,
            input_tokens: 30,
            output_tokens: 20,
            total_tokens: 50,
            cached_input_tokens: 3,
            cache_creation_input_tokens: 4,
        }],
    }];

    let html = render_dashboard(&[], &usage);

    assert!(html.contains("Daily token usage (UTC)"));
    assert!(html.contains("2026-05-11"));
    assert!(html.contains(r##"href="#session-work""##));
    assert!(html.contains("<td>50</td>"));
}

#[test]
fn cli_dashboard_includes_daily_token_usage() {
    let usage = vec![DailyTokenUsage {
        date: "2026-05-11".to_string(),
        calls: 1,
        input_tokens: 10,
        output_tokens: 5,
        total_tokens: 15,
        cached_input_tokens: 0,
        cache_creation_input_tokens: 0,
        sessions: vec![SessionTokenUsage {
            session_id: "work".to_string(),
            calls: 1,
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
        }],
    }];

    let text = render_cli_dashboard(&[], &usage, 80);

    assert!(text.contains("Daily token usage (UTC)"));
    assert!(text.contains("2026-05-11"));
    assert!(text.contains("session=work"));
    assert!(text.contains("15"));
}

#[test]
fn dashboard_preview_collapses_and_truncates_text() {
    assert_eq!(dashboard_preview("hello\nthere", 20), "hello there");
    assert_eq!(dashboard_preview("abcdef", 5), "ab...");
    assert_eq!(dashboard_preview("abcdef", 2), "ab");
}
