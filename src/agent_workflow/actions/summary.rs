use super::common::{contains_any, is_small_talk};
use crate::agent_workflow::weather_locations::is_boundary_punctuation;

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
