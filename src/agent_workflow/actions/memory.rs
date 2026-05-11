use super::common::{contains_any, is_small_talk};

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
