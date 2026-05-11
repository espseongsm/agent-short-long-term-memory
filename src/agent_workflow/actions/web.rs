use super::common::{contains_any, contains_current_info};

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
