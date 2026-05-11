use super::common::{contains_any, is_small_talk};

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
