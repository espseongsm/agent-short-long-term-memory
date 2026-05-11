pub(super) fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

pub(super) fn contains_current_info(value: &str, needles: &[&str]) -> bool {
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

pub(super) fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

pub(super) fn is_small_talk(prompt: &str) -> bool {
    matches!(
        prompt,
        "hi" | "hello" | "hey" | "안녕" | "안녕하세요" | "thanks" | "thank you" | "고마워"
    )
}

fn contains_ascii_word(value: &str, needle: &str) -> bool {
    value
        .split(|character: char| !character.is_ascii_alphabetic())
        .any(|word| word == needle)
}
