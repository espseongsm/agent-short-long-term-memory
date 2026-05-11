use super::*;
use agent_memory::{ChatEntry, ChatRole};

#[test]
fn automatic_actions_search_current_information() {
    assert_eq!(
        automatic_actions("What is the latest Rust release?"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: true,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_do_not_treat_know_as_now() {
    assert_eq!(
        automatic_actions("please let me know staten island"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_amplify_vague_requests() {
    assert_eq!(
        automatic_actions("make it better"),
        AutomaticActions {
            amplify: true,
            weather: false,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_leave_clear_stable_requests_alone() {
    assert_eq!(
        automatic_actions("Explain ownership in Rust"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_do_not_amplify_small_talk() {
    assert_eq!(
        automatic_actions("hello"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_leave_summary_requests_for_summary_agent() {
    assert_eq!(
        automatic_actions("summarize our conversation"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: false,
        }
    );
    assert!(should_auto_summarize("sum up what we talked about"));
    assert!(should_auto_summarize("이 대화 요약해줘"));
    assert!(should_auto_summarize("요약해줘"));
}

#[test]
fn summary_routing_ignores_external_text_summary_requests() {
    assert!(!should_auto_summarize("summarize this article"));
    assert!(!should_auto_summarize("summarize latest Rust release"));
    assert!(!should_auto_summarize("아래 문서 요약해줘"));
}

#[test]
fn automatic_actions_route_weather_before_web_search() {
    assert_eq!(
        automatic_actions("What is Seoul's current weather?"),
        AutomaticActions {
            amplify: false,
            weather: true,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_route_weather_follow_up_locations() {
    let history = vec![ChatEntry::new(ChatRole::User, "서울 오늘 날씨 알려줘")];

    assert_eq!(
        automatic_actions_for_chat(&history, "부산은?"),
        AutomaticActions {
            amplify: false,
            weather: true,
            web_search: false,
            long_term_memory: false,
        }
    );
    assert_eq!(
        automatic_actions_for_chat(&history, "what about Busan?"),
        AutomaticActions {
            amplify: false,
            weather: true,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_do_not_route_follow_up_without_weather_context() {
    let history = vec![ChatEntry::new(ChatRole::User, "안녕")];

    assert_eq!(
        automatic_actions_for_chat(&history, "부산은?"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_route_weather_location_corrections_before_web_search() {
    let history = vec![ChatEntry::new(
        ChatRole::Assistant,
        "서울 날씨는 이슬비입니다.",
    )];

    assert_eq!(
        automatic_actions_for_chat(&history, "서울이 아니라... 난 지금 퀸즈야"),
        AutomaticActions {
            amplify: false,
            weather: true,
            web_search: false,
            long_term_memory: false,
        }
    );
    assert_eq!(
        automatic_actions_for_chat(&history, "I'm in Queens, not Seoul"),
        AutomaticActions {
            amplify: false,
            weather: true,
            web_search: false,
            long_term_memory: false,
        }
    );
}

#[test]
fn automatic_actions_search_long_term_memory_only_for_memory_requests() {
    assert_eq!(
        automatic_actions("내가 요즘 어떤 메모를 하고 있어?"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: true,
        }
    );
    assert_eq!(
        automatic_actions("search long-term memory for Valkey notes"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: true,
        }
    );
    assert_eq!(
        automatic_actions("what are my recent notes about Valkey?"),
        AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: true,
        }
    );
    assert!(!should_auto_long_term_memory("remember to buy milk"));
    assert!(!should_auto_long_term_memory("Explain ownership in Rust"));
}
