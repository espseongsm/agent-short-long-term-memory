use agent_memory::{ChatEntry, ChatRole};

mod aliases;
#[cfg(test)]
mod tests;

use aliases::weather_location_alias;

const WEATHER_PRONOUN_LOCATIONS: &[&str] = &[
    "it",
    "of it",
    "there",
    "over there",
    "that",
    "that place",
    "this place",
    "그곳",
    "거기",
    "거기는",
    "그 지역",
    "그 동네",
];

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

    if let Some(location) = location_after_follow_up_prefix(&location) {
        return clean_weather_location(location);
    }

    if let Some(location) = location_after_self_location_statement(&location) {
        return clean_weather_location(location);
    }

    if let Some(location) = location_after_weather_preposition(&location) {
        return clean_weather_location(location);
    }

    let removable = [
        "what is",
        "what's",
        "how is",
        "how's",
        "of",
        "current",
        "weather",
        "forecast",
        "temperature",
        "the",
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

pub fn weather_location_query_for_chat(history: &[ChatEntry], prompt: &str) -> String {
    let location = weather_location_query(prompt);

    if is_pronoun_weather_location(&location) || is_pronoun_weather_prompt(prompt) {
        recent_reference_location(history).unwrap_or(location)
    } else {
        location
    }
}

fn is_pronoun_weather_location(location: &str) -> bool {
    let location = location.trim().to_lowercase();

    WEATHER_PRONOUN_LOCATIONS.contains(&location.as_str())
}

fn is_pronoun_weather_prompt(prompt: &str) -> bool {
    contains_weather_pronoun(&prompt.to_lowercase())
}

fn recent_reference_location(history: &[ChatEntry]) -> Option<String> {
    history
        .iter()
        .rev()
        .take(8)
        .find_map(reference_location_from_entry)
}

fn reference_location_from_entry(entry: &ChatEntry) -> Option<String> {
    match entry.role {
        ChatRole::User => reference_location_from_user_text(&entry.content),
        ChatRole::Assistant => reference_location_from_assistant_text(&entry.content),
    }
}

fn reference_location_from_user_text(content: &str) -> Option<String> {
    let content = first_content_line(content);
    let lower = content.to_lowercase();
    let markers = [
        "let me know ",
        "tell me about ",
        "tell me ",
        "what is ",
        "what's ",
        "where is ",
        "where's ",
        "show me ",
        "explain ",
        "about ",
    ];

    markers.iter().find_map(|marker| {
        lower.rfind(marker).and_then(|index| {
            let start = index + marker.len();
            clean_reference_location(&content[start..])
        })
    })
}

fn reference_location_from_assistant_text(content: &str) -> Option<String> {
    let line = first_content_line(content);
    let lower = line.to_lowercase();
    let markers = [" is ", " are ", ":", "은", "는"];

    markers
        .iter()
        .filter_map(|marker| lower.find(marker))
        .min()
        .and_then(|index| clean_reference_location(&line[..index]))
}

fn first_content_line(content: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(content)
        .trim_start_matches(['-', '*'])
        .trim()
        .to_string()
}

fn clean_reference_location(candidate: &str) -> Option<String> {
    let candidate = candidate
        .trim()
        .trim_matches(is_boundary_punctuation)
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    let candidate = clean_weather_location(&candidate);

    is_plausible_reference_location(&candidate).then_some(candidate)
}

fn is_plausible_reference_location(candidate: &str) -> bool {
    let lower = candidate.to_lowercase();

    !candidate.is_empty()
        && !is_non_location_weather_residue(candidate)
        && !is_pronoun_weather_location(candidate)
        && candidate.chars().count() <= 80
        && candidate.split_whitespace().count() <= 6
        && candidate.chars().any(char::is_alphabetic)
        && !matches!(
            lower.as_str(),
            "i" | "i can"
                | "i can help"
                | "key points"
                | "location"
                | "known for"
                | "transportation"
                | "famous places"
        )
}

fn contains_weather_pronoun(prompt: &str) -> bool {
    WEATHER_PRONOUN_LOCATIONS
        .iter()
        .any(|pronoun| contains_phrase(prompt, pronoun))
}

fn contains_phrase(value: &str, phrase: &str) -> bool {
    if phrase
        .chars()
        .any(|character| !character.is_ascii_alphabetic())
    {
        return value.contains(phrase);
    }

    value
        .split(|character: char| !character.is_ascii_alphabetic())
        .any(|word| word == phrase)
}

fn location_after_follow_up_prefix(location: &str) -> Option<&str> {
    let lower = location.to_lowercase();
    let prefixes = ["what about ", "how about ", "and ", "그럼", "그러면"];

    for prefix in prefixes {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let start = location.len() - rest.len();
            return Some(location[start..].trim_start_matches(is_boundary_punctuation));
        }
    }

    None
}

fn location_after_self_location_statement(location: &str) -> Option<&str> {
    let lower = location.to_lowercase();
    let prefixes = [
        "i'm currently in ",
        "i am currently in ",
        "im currently in ",
        "i'm in ",
        "i am in ",
        "im in ",
        "i'm at ",
        "i am at ",
        "im at ",
        "currently in ",
        "난 지금 ",
        "나는 지금 ",
        "나 지금 ",
        "저는 지금 ",
        "저 지금 ",
        "난 ",
        "나는 ",
        "나 ",
        "저는 ",
        "저 ",
    ];

    prefixes
        .iter()
        .filter_map(|prefix| {
            lower.rfind(prefix).and_then(|index| {
                is_self_location_marker_boundary(location, index)
                    .then_some((index, index + prefix.len()))
            })
        })
        .max_by_key(|(index, start)| (*index, *start))
        .and_then(|(_, start)| {
            let location = &location[start..];

            if location.trim().is_empty() {
                None
            } else {
                Some(location)
            }
        })
}

fn is_self_location_marker_boundary(location: &str, index: usize) -> bool {
    if index == 0 {
        return true;
    }

    location[..index]
        .chars()
        .next_back()
        .is_some_and(|character| character.is_whitespace() || is_boundary_punctuation(character))
}

fn location_after_weather_preposition(location: &str) -> Option<&str> {
    let lower = location.to_lowercase();

    [" in ", " for ", " at ", " of "]
        .iter()
        .filter_map(|marker| {
            lower
                .rfind(marker)
                .map(|index| (index, index + marker.len()))
        })
        .max_by_key(|(index, _)| *index)
        .and_then(|(_, start)| {
            let location = &location[start..];

            if location.trim().is_empty() {
                None
            } else {
                Some(location)
            }
        })
}

fn clean_weather_location(location: &str) -> String {
    let mut location = location
        .trim()
        .trim_matches(is_boundary_punctuation)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    location = strip_leading_weather_location_noise(&location).to_string();
    location = strip_location_correction_tail(&location).to_string();
    location = strip_trailing_weather_location_noise(&location).to_string();

    if let Some(without_possessive) = location.strip_suffix("'s") {
        location = without_possessive.to_string();
    }

    location = strip_trailing_korean_copula(&location).to_string();
    location = strip_trailing_korean_particle(&location).to_string();
    location = weather_location_alias(&location).to_string();

    if location.is_empty() || is_non_location_weather_residue(&location) {
        "Seoul".to_string()
    } else {
        location
    }
}

fn strip_location_correction_tail(location: &str) -> &str {
    let lower = location.to_lowercase();
    let markers = [", not ", " not ", ", instead", " instead", "아니라"];

    markers
        .iter()
        .filter_map(|marker| lower.find(marker))
        .min()
        .map(|index| {
            location[..index]
                .trim_end_matches(is_boundary_punctuation)
                .trim_end()
        })
        .unwrap_or(location)
}

fn is_non_location_weather_residue(location: &str) -> bool {
    let location = location.trim().to_lowercase();

    location.is_empty()
        || matches!(
            location.as_str(),
            "a" | "an"
                | "the"
                | "is"
                | "in"
                | "for"
                | "at"
                | "of"
                | "it"
                | "of it"
                | "there"
                | "how"
                | "how is"
                | "how's"
                | "hello"
                | "hello there"
                | "hi"
                | "hey"
                | "good morning"
                | "good afternoon"
                | "good evening"
                | "what"
                | "what is"
                | "what's"
                | "안녕"
                | "안녕하세요"
                | "은"
                | "는"
                | "이"
                | "가"
                | "을"
                | "를"
        )
}

fn strip_leading_weather_location_noise(location: &str) -> &str {
    for prefix in [
        "hello there",
        "good morning",
        "good afternoon",
        "good evening",
    ] {
        if location.to_lowercase().starts_with(prefix) {
            let rest = location[prefix.len()..].trim_start();
            if !rest.is_empty() {
                return rest;
            }
        }
    }

    let Some((first, rest)) = location.split_once(' ') else {
        return location;
    };

    if matches!(
        first.to_lowercase().as_str(),
        "in" | "for" | "at" | "of" | "hello" | "hi" | "hey" | "안녕" | "안녕하세요"
    ) {
        rest.trim_start()
    } else {
        location
    }
}

fn strip_trailing_weather_location_noise(location: &str) -> &str {
    let Some((rest, last)) = location.rsplit_once(' ') else {
        return location;
    };

    if matches!(
        last.to_lowercase().as_str(),
        "today" | "now" | "currently" | "please"
    ) {
        rest.trim_end()
    } else {
        location
    }
}

fn strip_trailing_korean_copula(location: &str) -> &str {
    for suffix in [
        "에 있어요",
        "에 있어",
        "입니다",
        "이에요",
        "예요",
        "이야",
        "야",
    ] {
        if let Some(location) = location.strip_suffix(suffix) {
            return location.trim_end();
        }
    }

    location
}

fn strip_trailing_korean_particle(location: &str) -> &str {
    if location.chars().count() <= 1 || !ends_with_korean_topic_particle(location) {
        return location;
    }

    let particle_start = location
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(location.len());

    location[..particle_start].trim_end()
}

pub(super) fn ends_with_korean_topic_particle(value: &str) -> bool {
    matches!(
        value.chars().next_back(),
        Some('은' | '는' | '이' | '가' | '을' | '를')
    )
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

pub(super) fn is_boundary_punctuation(character: char) -> bool {
    matches!(
        character,
        '?' | '!' | '.' | ',' | ':' | ';' | '"' | '\'' | '`' | '“' | '”'
    )
}
