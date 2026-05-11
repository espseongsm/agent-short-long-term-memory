use agent_memory::ChatEntry;

use super::common::{contains_any, is_small_talk, starts_with_any};
use crate::agent_workflow::weather_locations::{
    ends_with_korean_topic_particle, is_boundary_punctuation, weather_location_query,
};

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

pub(super) fn has_recent_weather_context(history: &[ChatEntry]) -> bool {
    history
        .iter()
        .rev()
        .take(6)
        .any(|entry| is_weather_context_text(&entry.content))
}

pub(super) fn is_weather_location_follow_up(prompt: &str) -> bool {
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
