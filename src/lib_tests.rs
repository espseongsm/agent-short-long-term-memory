use super::*;

#[test]
fn normalizes_namespace_before_building_keys() {
    let namespace = normalize_namespace("agent:short-term:");

    assert_eq!(
        namespaced_key(&namespace, "session"),
        "agent:short-term:session"
    );
}

#[test]
fn filters_chat_entries_case_insensitively() {
    let entries = vec![
        ChatEntry::new(ChatRole::User, "Remember the Valkey setup"),
        ChatEntry::new(
            ChatRole::Assistant,
            "The server URL is redis://127.0.0.1:6379/",
        ),
    ];

    assert_eq!(
        search_entries(entries, "valkey"),
        vec![ChatEntry::new(ChatRole::User, "Remember the Valkey setup")]
    );
}

#[test]
fn creates_chat_entries_with_user_session_and_timestamp() {
    let before = unix_timestamp_seconds();
    let entry = ChatEntry::for_session("soonmo", "work", ChatRole::User, "hello");
    let after = unix_timestamp_seconds();

    assert_eq!(entry.user_id, "soonmo");
    assert_eq!(entry.session_id, "work");
    assert!(entry.timestamp >= before);
    assert!(entry.timestamp <= after);
}

#[test]
fn reads_legacy_chat_entries_without_metadata() {
    let entry: ChatEntry = serde_json::from_str(r#"{"role":"user","content":"legacy"}"#).unwrap();

    assert_eq!(entry, ChatEntry::new(ChatRole::User, "legacy"));
}

#[test]
fn saves_system_prompt_as_yaml() {
    let prompt_dir = std::env::temp_dir().join(format!(
        "agent-memory-system-prompt-test-{}-{}",
        unix_timestamp_seconds(),
        std::process::id()
    ));

    let path = save_system_prompt_yaml(&prompt_dir, "system instructions").unwrap();
    let yaml = std::fs::read_to_string(path).unwrap();
    let saved: SystemPromptArchiveEntry = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(
        saved,
        SystemPromptArchiveEntry {
            role: "system".to_string(),
            prompt: "system instructions".to_string(),
        }
    );
    assert!(prompt_dir.join("system.yaml").exists());

    std::fs::remove_dir_all(prompt_dir).unwrap();
}

#[test]
#[ignore = "requires a Valkey server at VALKEY_URL or redis://127.0.0.1:6379/"]
fn remembers_recalls_and_forgets_with_valkey() {
    let url = std::env::var("VALKEY_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
    let mut memory = ShortTermMemory::connect(&url, "agent:test").unwrap();

    memory
        .remember("integration", "short lived value", 30)
        .unwrap();

    assert_eq!(
        memory.recall("integration").unwrap(),
        Some("short lived value".to_string())
    );
    assert!(memory.forget("integration").unwrap());
    assert_eq!(memory.recall("integration").unwrap(), None);
}

#[test]
#[ignore = "requires a Valkey server at VALKEY_URL or redis://127.0.0.1:6379/"]
fn stores_and_searches_chat_history_with_valkey() {
    let url = std::env::var("VALKEY_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
    let mut memory = ShortTermMemory::connect(&url, "agent:test").unwrap();

    memory.forget("chat:integration-session").unwrap();
    let entry = ChatEntry::with_metadata(
        "soonmo",
        "integration-session",
        1,
        ChatRole::User,
        "Searchable Valkey message",
    );

    memory
        .append_chat_entry("integration-session", entry.clone(), 10, 30)
        .unwrap();

    assert_eq!(
        memory
            .search_chat_history("integration-session", "valkey")
            .unwrap(),
        vec![entry]
    );
    assert!(
        memory
            .chat_session_summaries(10)
            .unwrap()
            .iter()
            .any(|summary| summary.session_id == "integration-session"
                && summary.entry_count == 1
                && summary.last_content == "Searchable Valkey message")
    );

    memory.forget("chat:integration-session").unwrap();
}

#[test]
fn builds_chat_session_summary_from_entries() {
    let entries = vec![
        ChatEntry::with_metadata("soonmo", "work", 10, ChatRole::User, "hello"),
        ChatEntry::with_metadata("soonmo", "work", 20, ChatRole::Assistant, "hi there"),
    ];

    assert_eq!(
        chat_session_summary("work".to_string(), &entries),
        Some(ChatSessionSummary {
            session_id: "work".to_string(),
            entry_count: 2,
            user_ids: vec!["soonmo".to_string()],
            first_timestamp: 10,
            last_timestamp: 20,
            last_role: ChatRole::Assistant,
            last_content: "hi there".to_string(),
        })
    );
    assert_eq!(chat_session_summary("empty".to_string(), &[]), None);
}
