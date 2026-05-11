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
fn brand_height_uses_large_compact_or_hidden_layout() {
    assert_eq!(brand_height(24), 7);
    assert_eq!(brand_height(16), 3);
    assert_eq!(brand_height(15), 0);
}

#[test]
fn brand_lines_use_compact_and_large_labels() {
    let compact = brand_lines(3);
    assert_eq!(compact.len(), 1);
    assert_eq!(compact[0].spans[0].content, "RUST");
    assert_eq!(compact[0].spans[2].content, "RIG");

    let large = brand_lines(7);
    assert_eq!(large.len(), 5);
    assert!(large[0].spans[0].content.contains("RRRR"));
    assert!(large[0].spans[2].content.contains("GGG"));
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
fn formats_elapsed_time_with_two_decimal_places() {
    assert_eq!(format_elapsed_time(Duration::from_millis(2120)), "2.12s");
    assert_eq!(format_elapsed_time(Duration::from_millis(5)), "0.01s");
}

#[test]
fn formats_token_usage() {
    assert_eq!(token_usage_label(TokenUsage::default()), "n/a");
    assert_eq!(
        token_usage_label(TokenUsage {
            input_tokens: 12,
            output_tokens: 8,
            total_tokens: 20,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
        }),
        "in 12 / out 8 / total 20"
    );
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
fn conversation_transcript_includes_actions_between_messages() {
    let entries = vec![
        ChatEntry::new(ChatRole::User, "hello"),
        ChatEntry::new(ChatRole::Assistant, "hi"),
    ];
    let actions = vec![AgentAction {
        after_entry_count: 1,
        content: "LLM: received model response".to_string(),
    }];

    assert_eq!(
        conversation_transcript(&entries, &actions),
        "user:\nhello\n\nassistant action:\nLLM: received model response\n\nassistant:\nhi"
    );
}

#[test]
fn error_chain_includes_causes() {
    let error = anyhow::anyhow!("root").context("outer");

    assert_eq!(format_error_chain(&error), "error: outer | root");
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
