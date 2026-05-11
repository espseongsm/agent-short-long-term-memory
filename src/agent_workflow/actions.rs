use agent_memory::ChatEntry;

use super::weather_locations::{
    ends_with_korean_topic_particle, is_boundary_punctuation, weather_location_query,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AutomaticActions {
    pub amplify: bool,
    pub weather: bool,
    pub web_search: bool,
    pub long_term_memory: bool,
}

pub fn automatic_actions(prompt: &str) -> AutomaticActions {
    if should_auto_summarize(prompt) {
        return AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: false,
        };
    }

    let weather = should_auto_weather(prompt);
    let long_term_memory_candidate = should_auto_long_term_memory(prompt);
    let web_search = !weather && !long_term_memory_candidate && should_auto_web_search(prompt);
    let long_term_memory = !weather && !web_search && long_term_memory_candidate;

    AutomaticActions {
        amplify: !weather && !web_search && !long_term_memory && should_auto_amplify(prompt),
        weather,
        web_search,
        long_term_memory,
    }
}

pub fn automatic_actions_for_chat(history: &[ChatEntry], prompt: &str) -> AutomaticActions {
    let mut actions = automatic_actions(prompt);

    if !actions.weather
        && has_recent_weather_context(history)
        && is_weather_location_follow_up(prompt)
    {
        actions.amplify = false;
        actions.weather = true;
        actions.web_search = false;
        actions.long_term_memory = false;

        return actions;
    }

    if actions.weather || actions.web_search {
        return actions;
    }

    actions
}

pub fn should_auto_long_term_memory(prompt: &str) -> bool {
    let prompt = prompt.trim().to_lowercase();
    if prompt.is_empty() || is_small_talk(&prompt) {
        return false;
    }

    if contains_any(
        &prompt,
        &[
            "long term memory",
            "long-term memory",
            "local markdown",
            "markdown memory",
            "장기 메모리",
            "장기 기억",
            "장기기억",
            "롱텀 메모리",
            "로컬 마크다운",
        ],
    ) {
        return true;
    }

    let memory_targets = [
        "my note",
        "my notes",
        "my memo",
        "my memos",
        "my journal",
        "my diary",
        "notes",
        "memos",
        "journals",
        "diaries",
        "obsidian",
        "markdown",
        "local memory",
        "pgvector",
        "내 메모",
        "내 노트",
        "내 기록",
        "내 일기",
        "나의 메모",
        "나의 노트",
        "메모",
        "노트",
        "기록",
        "일기",
        "옵시디언",
        "마크다운",
    ];
    let retrieval_intents = [
        "what",
        "which",
        "where",
        "find",
        "search",
        "look up",
        "lookup",
        "recall",
        "show",
        "summarize",
        "summary",
        "about",
        "say about",
        "알려",
        "찾",
        "검색",
        "조회",
        "꺼내",
        "요약",
        "정리",
        "뭐",
        "무엇",
        "어떤",
        "어디",
        "최근",
        "요즘",
        "관해",
        "대해",
    ];

    contains_any(&prompt, &memory_targets) && contains_any(&prompt, &retrieval_intents)
}

pub fn should_auto_weather(prompt: &str) -> bool {
    let prompt = prompt.trim().to_lowercase();
    if prompt.is_empty() {
        return false;
    }

    contains_any(
        &prompt,
        &["weather", "forecast", "temperature", "날씨", "기온"],
    )
}

fn has_recent_weather_context(history: &[ChatEntry]) -> bool {
    history
        .iter()
        .rev()
        .take(6)
        .any(|entry| is_weather_context_text(&entry.content))
}

fn is_weather_context_text(content: &str) -> bool {
    let content = content.to_lowercase();

    contains_any(
        &content,
        &[
            "weather",
            "forecast",
            "temperature",
            "open-meteo",
            "날씨",
            "기온",
            "온도",
        ],
    )
}

fn is_weather_location_follow_up(prompt: &str) -> bool {
    let prompt = prompt.trim();
    if prompt.is_empty() || is_small_talk(&prompt.to_lowercase()) {
        return false;
    }

    let location = weather_location_query(prompt);
    let lower_location = location.to_lowercase();
    if location.is_empty()
        || is_non_location_follow_up(&lower_location)
        || location.chars().count() > 60
        || location.split_whitespace().count() > 4
    {
        return false;
    }

    let lower_prompt = prompt.to_lowercase();
    let trimmed = prompt.trim_end_matches(is_boundary_punctuation);

    prompt.ends_with('?')
        || starts_with_any(
            &lower_prompt,
            &[
                "what about",
                "how about",
                "and ",
                "i'm ",
                "i am ",
                "im ",
                "그럼",
                "그러면",
                "난 ",
                "나는 ",
                "나 ",
                "저는 ",
                "저 ",
            ],
        )
        || contains_any(&lower_prompt, &[" not ", "i'm in ", "i am in ", "im in "])
        || contains_any(
            prompt,
            &[
                "아니라",
                "난 지금",
                "나는 지금",
                "나 지금",
                "저는 지금",
                "저 지금",
            ],
        )
        || ends_with_korean_topic_particle(trimmed)
}

fn is_non_location_follow_up(value: &str) -> bool {
    matches!(
        value,
        "why"
            | "how"
            | "what"
            | "when"
            | "where"
            | "who"
            | "which"
            | "왜"
            | "어떻게"
            | "뭐"
            | "무엇"
            | "언제"
            | "어디"
            | "누구"
            | "그거"
            | "이거"
            | "저거"
    )
}

fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

pub fn should_auto_web_search(prompt: &str) -> bool {
    let prompt = prompt.trim().to_lowercase();
    if prompt.is_empty() {
        return false;
    }

    let explicit_search = [
        "search the web",
        "web search",
        "look up",
        "lookup",
        "browse",
        "google",
        "internet",
        "online",
        "검색",
        "찾아봐",
    ];
    let current_info = [
        "latest",
        "current",
        "today",
        "tonight",
        "tomorrow",
        "yesterday",
        "recent",
        "news",
        "now",
        "this week",
        "this month",
        "price",
        "stock",
        "weather",
        "release date",
        "version",
        "최신",
        "현재",
        "오늘",
        "내일",
        "어제",
        "최근",
        "뉴스",
        "지금",
        "가격",
        "주가",
        "날씨",
        "버전",
    ];

    contains_any(&prompt, &explicit_search) || contains_current_info(&prompt, &current_info)
}

pub fn should_auto_amplify(prompt: &str) -> bool {
    let prompt = prompt.trim().to_lowercase();
    if prompt.is_empty() || is_small_talk(&prompt) {
        return false;
    }

    let word_count = prompt.split_whitespace().count();
    let vague_actions = [
        "make", "fix", "improve", "update", "change", "refactor", "clean", "help", "do", "만들",
        "고쳐", "수정", "개선", "바꿔", "도와", "해줘",
    ];
    let vague_targets = [
        "it", "this", "that", "thing", "stuff", "better", "good", "nice", "이거", "그거", "저거",
        "좋게", "잘",
    ];

    if word_count <= 3 {
        return contains_any(&prompt, &vague_actions) || contains_any(&prompt, &vague_targets);
    }

    word_count <= 6
        && contains_any(&prompt, &vague_actions)
        && contains_any(&prompt, &vague_targets)
}

pub fn should_auto_summarize(prompt: &str) -> bool {
    let prompt = prompt.trim();
    let lower = prompt.to_lowercase();
    if lower.is_empty() || is_small_talk(&lower) || !has_summary_intent(&lower) {
        return false;
    }

    if contains_any(&lower, &conversation_summary_targets()) {
        return true;
    }

    if contains_any(&lower, &external_summary_targets()) {
        return false;
    }

    is_short_summary_request(&lower)
}

fn has_summary_intent(value: &str) -> bool {
    contains_any(
        value,
        &[
            "summarize",
            "summary",
            "sum up",
            "recap",
            "tl;dr",
            "요약",
            "정리",
        ],
    )
}

fn conversation_summary_targets() -> [&'static str; 17] {
    [
        "conversation",
        "chat",
        "history",
        "session",
        "what we discussed",
        "what we talked",
        "our discussion",
        "so far",
        "previous",
        "above",
        "대화",
        "채팅",
        "기록",
        "지금까지",
        "방금",
        "앞에서",
        "이전",
    ]
}

fn external_summary_targets() -> [&'static str; 17] {
    [
        "this article",
        "this document",
        "this text",
        "this file",
        "this code",
        "following",
        "below",
        "article",
        "document",
        "file",
        "code",
        "기사",
        "문서",
        "파일",
        "코드",
        "본문",
        "아래",
    ]
}

fn is_short_summary_request(value: &str) -> bool {
    matches!(
        value.trim_matches(is_boundary_punctuation),
        "summary"
            | "summarize"
            | "summarize please"
            | "please summarize"
            | "sum up"
            | "recap"
            | "recap please"
            | "tl;dr"
            | "요약"
            | "요약해"
            | "요약해줘"
            | "정리"
            | "정리해"
            | "정리해줘"
    )
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn contains_current_info(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| {
        if needle
            .chars()
            .all(|character| character.is_ascii_alphabetic())
        {
            contains_ascii_word(value, needle)
        } else {
            value.contains(needle)
        }
    })
}

fn contains_ascii_word(value: &str, needle: &str) -> bool {
    value
        .split(|character: char| !character.is_ascii_alphabetic())
        .any(|word| word == needle)
}

fn is_small_talk(prompt: &str) -> bool {
    matches!(
        prompt,
        "hi" | "hello" | "hey" | "안녕" | "안녕하세요" | "thanks" | "thank you" | "고마워"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_memory::ChatRole;

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
}
