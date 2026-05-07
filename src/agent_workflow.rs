use crate::{
    llm::{LlmClient, ReasoningEffort},
    long_term_memory,
    weather::WeatherClient,
    web_search::{DEFAULT_WEB_SEARCH_LIMIT, WebSearchClient},
};
use agent_memory::ChatEntry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AutomaticActions {
    pub amplify: bool,
    pub weather: bool,
    pub web_search: bool,
}

pub fn automatic_actions(prompt: &str) -> AutomaticActions {
    let weather = should_auto_weather(prompt);
    let web_search = !weather && should_auto_web_search(prompt);

    AutomaticActions {
        amplify: !weather && !web_search && should_auto_amplify(prompt),
        weather,
        web_search,
    }
}

pub fn prompt_with_amplification(original_prompt: &str, amplified_prompt: &str) -> String {
    format!(
        "Original user request:\n{original_prompt}\n\nAmplified request:\n{amplified_prompt}\n\nAnswer the amplified request while preserving the original intent."
    )
}

pub fn prompt_with_web_search_context(prompt: &str, web_search_results: &str) -> String {
    format!(
        "User request:\n{prompt}\n\nAutomatic web search context:\n{web_search_results}\n\nUse the web search context when it is relevant. If it is not useful, say so briefly and answer from the best available context."
    )
}

pub fn prompt_with_weather_context(prompt: &str, weather_report: &str) -> String {
    format!(
        "User request:\n{prompt}\n\nAutomatic weather context:\n{weather_report}\n\nUse the weather context to answer the user's weather question."
    )
}

pub fn prompt_with_long_term_context(prompt: &str, long_term_context: &str) -> String {
    format!(
        "User request:\n{prompt}\n\nLong-term memory context from local Markdown files:\n{long_term_context}\n\nUse the long-term memory context when it is relevant. Ignore it if it is not useful."
    )
}

pub async fn automatic_chat_prompt(
    pgvector_url: Option<&str>,
    history: &[ChatEntry],
    prompt: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    amplifier_preamble: &'static str,
) -> String {
    let actions = automatic_actions(prompt);
    let mut prompt_for_llm = prompt.to_string();

    if actions.amplify {
        eprintln!("auto request amplifier: expanding vague request");
        match LlmClient::from_env(model.to_string(), amplifier_preamble, reasoning_effort) {
            Ok(amplifier) => match amplifier.chat(history, prompt).await {
                Ok(amplified) => {
                    eprintln!("auto request amplifier: produced amplified request");
                    prompt_for_llm = prompt_with_amplification(prompt, &amplified);
                }
                Err(error) => eprintln!("auto request amplifier failed: {error:#}"),
            },
            Err(error) => {
                eprintln!("auto request amplifier init failed: {error:#}");
            }
        }
    }

    if actions.weather {
        let location = weather_location_query(prompt);
        eprintln!("auto weather: fetching current weather for `{location}`");

        match WeatherClient::from_env() {
            Ok(weather) => match weather.current_weather(&location).await {
                Ok(report) => {
                    eprintln!("auto weather: received current weather");
                    prompt_for_llm =
                        prompt_with_weather_context(&prompt_for_llm, &report.to_markdown());
                }
                Err(error) => eprintln!("auto weather failed: {error:#}"),
            },
            Err(error) => eprintln!("auto weather init failed: {error:#}"),
        }
    }

    if actions.web_search {
        eprintln!("auto web search: searching for current context");
        match WebSearchClient::from_env() {
            Ok(web_search) => match web_search.search(prompt, DEFAULT_WEB_SEARCH_LIMIT).await {
                Ok(results) => {
                    eprintln!("auto web search: received results");
                    prompt_for_llm =
                        prompt_with_web_search_context(&prompt_for_llm, &results.to_markdown());
                }
                Err(error) => eprintln!("auto web search failed: {error:#}"),
            },
            Err(error) => eprintln!("auto web search init failed: {error:#}"),
        }
    }

    if let Some(long_term_context) = automatic_long_term_context(pgvector_url, prompt).await {
        prompt_for_llm = prompt_with_long_term_context(&prompt_for_llm, &long_term_context);
    }

    prompt_for_llm
}

async fn automatic_long_term_context(pgvector_url: Option<&str>, prompt: &str) -> Option<String> {
    let pgvector_url = pgvector_url?;

    eprintln!("auto long-term memory: searching local markdown context");
    let long_term = match long_term_memory::connect(pgvector_url).await {
        Ok(long_term) => long_term,
        Err(error) => {
            eprintln!("auto long-term memory init failed: {error:#}");
            return None;
        }
    };
    let results = match long_term.search(prompt, 3).await {
        Ok(results) => results,
        Err(error) => {
            eprintln!("auto long-term memory search failed: {error:#}");
            return None;
        }
    };

    if results.is_empty() {
        return None;
    }

    eprintln!("auto long-term memory: found context");
    Some(long_term_memory::LongTermSearchResult::to_markdown(
        &results,
    ))
}

pub fn weather_location_query(prompt: &str) -> String {
    let mut location = prompt
        .trim()
        .trim_matches(is_boundary_punctuation)
        .to_string();
    let phrase_prefixes = [
        "what is the weather in",
        "what's the weather in",
        "weather in",
        "current weather in",
        "forecast for",
        "temperature in",
    ];
    let lower = location.to_lowercase();

    for prefix in phrase_prefixes {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let start = location.len() - rest.len();
            return clean_weather_location(&location[start..]);
        }
    }

    let removable = [
        "what is",
        "what's",
        "current",
        "weather",
        "forecast",
        "temperature",
        "today",
        "now",
        "please",
        "tell me",
        "show me",
        "현재",
        "오늘",
        "지금",
        "날씨",
        "기온",
        "알려줘",
        "어때",
    ];

    for word in removable {
        location = replace_case_insensitive(&location, word, " ");
    }

    clean_weather_location(&location)
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

    contains_any(&prompt, &explicit_search) || contains_any(&prompt, &current_info)
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

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn clean_weather_location(location: &str) -> String {
    let mut location = location
        .trim()
        .trim_matches(is_boundary_punctuation)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if let Some(without_possessive) = location.strip_suffix("'s") {
        location = without_possessive.to_string();
    }

    if location.is_empty() {
        "Seoul".to_string()
    } else {
        location
    }
}

fn replace_case_insensitive(value: &str, needle: &str, replacement: &str) -> String {
    let lower_value = value.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let mut result = String::new();
    let mut cursor = 0;

    while let Some(relative_index) = lower_value[cursor..].find(&lower_needle) {
        let index = cursor + relative_index;
        result.push_str(&value[cursor..index]);
        result.push_str(replacement);
        cursor = index + lower_needle.len();
    }

    result.push_str(&value[cursor..]);
    result
}

fn is_boundary_punctuation(character: char) -> bool {
    matches!(
        character,
        '?' | '!' | '.' | ',' | ':' | ';' | '"' | '\'' | '`' | '“' | '”'
    )
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

    #[test]
    fn automatic_actions_search_current_information() {
        assert_eq!(
            automatic_actions("What is the latest Rust release?"),
            AutomaticActions {
                amplify: false,
                weather: false,
                web_search: true,
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
            }
        );
    }

    #[test]
    fn automatic_actions_route_weather_before_web_search() {
        assert_eq!(
            automatic_actions("What is Seoul's current weather?"),
            AutomaticActions {
                amplify: false,
                weather: true,
                web_search: false,
            }
        );
    }

    #[test]
    fn extracts_weather_locations() {
        assert_eq!(weather_location_query("Seoul current weather"), "Seoul");
        assert_eq!(weather_location_query("weather in Seoul"), "Seoul");
        assert_eq!(
            weather_location_query("What is Seoul's current weather?"),
            "Seoul"
        );
        assert_eq!(weather_location_query("서울 현재 날씨"), "서울");
    }

    #[test]
    fn prompt_enrichment_preserves_original_request() {
        let prompt = prompt_with_amplification("fix it", "Fix the login button alignment.");

        assert!(prompt.contains("Original user request:\nfix it"));
        assert!(prompt.contains("Amplified request:\nFix the login button alignment."));
    }
}
