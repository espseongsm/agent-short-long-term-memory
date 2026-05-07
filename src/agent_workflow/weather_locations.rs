use agent_memory::{ChatEntry, ChatRole};

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
                | "what"
                | "what is"
                | "what's"
                | "은"
                | "는"
                | "이"
                | "가"
                | "을"
                | "를"
        )
}

fn strip_leading_weather_location_noise(location: &str) -> &str {
    let Some((first, rest)) = location.split_once(' ') else {
        return location;
    };

    if matches!(first.to_lowercase().as_str(), "in" | "for" | "at" | "of") {
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

fn weather_location_alias(location: &str) -> &str {
    match location {
        "서울" | "서울시" | "서울특별시" => "Seoul",
        "부산" | "부산시" | "부산광역시" => "Busan",
        "인천" | "인천시" | "인천광역시" => "Incheon",
        "대구" | "대구시" | "대구광역시" => "Daegu",
        "대전" | "대전시" | "대전광역시" => "Daejeon",
        "광주" | "광주시" | "광주광역시" => "Gwangju",
        "울산" | "울산시" | "울산광역시" => "Ulsan",
        "세종" | "세종시" | "세종특별자치시" => "Sejong",
        "제주" | "제주시" => "Jeju City",
        "서귀포" | "서귀포시" => "Seogwipo",
        "수원" | "수원시" => "Suwon",
        "성남" | "성남시" => "Seongnam",
        "용인" | "용인시" => "Yongin",
        "고양" | "고양시" => "Goyang",
        "창원" | "창원시" => "Changwon",
        "청주" | "청주시" => "Cheongju",
        "전주" | "전주시" => "Jeonju",
        "천안" | "천안시" => "Cheonan",
        "포항" | "포항시" => "Pohang",
        "춘천" | "춘천시" => "Chuncheon",
        "강릉" | "강릉시" => "Gangneung",
        "뉴욕" | "뉴욕시" => "New York",
        "퀸즈" | "퀸즈구" => "Queens",
        "워싱턴" | "워싱턴디씨" | "워싱턴 dc" | "워싱턴 d.c." => "Washington DC",
        "로스앤젤레스" | "엘에이" | "la" => "Los Angeles",
        "샌프란시스코" => "San Francisco",
        "시카고" => "Chicago",
        "라스베이거스" | "라스베가스" => "Las Vegas",
        "런던" => "London",
        "파리" => "Paris",
        "베를린" => "Berlin",
        "로마" => "Rome",
        "마드리드" => "Madrid",
        "바르셀로나" => "Barcelona",
        "리스본" => "Lisbon",
        "암스테르담" => "Amsterdam",
        "브뤼셀" => "Brussels",
        "빈" | "비엔나" => "Vienna",
        "취리히" => "Zurich",
        "제네바" => "Geneva",
        "프라하" => "Prague",
        "부다페스트" => "Budapest",
        "바르샤바" => "Warsaw",
        "코펜하겐" => "Copenhagen",
        "스톡홀름" => "Stockholm",
        "오슬로" => "Oslo",
        "헬싱키" => "Helsinki",
        "더블린" => "Dublin",
        "모스크바" => "Moscow",
        "이스탄불" => "Istanbul",
        "두바이" => "Dubai",
        "아부다비" => "Abu Dhabi",
        "도하" => "Doha",
        "리야드" => "Riyadh",
        "카이로" => "Cairo",
        "케이프타운" => "Cape Town",
        "요하네스버그" => "Johannesburg",
        "나이로비" => "Nairobi",
        "방콕" => "Bangkok",
        "하노이" => "Hanoi",
        "호치민" | "호찌민" => "Ho Chi Minh City",
        "싱가포르" => "Singapore",
        "쿠알라룸푸르" => "Kuala Lumpur",
        "자카르타" => "Jakarta",
        "마닐라" => "Manila",
        "타이베이" | "타이페이" => "Taipei",
        "홍콩" => "Hong Kong",
        "마카오" => "Macau",
        "베이징" | "북경" => "Beijing",
        "상하이" | "상해" => "Shanghai",
        "광저우" => "Guangzhou",
        "선전" | "심천" => "Shenzhen",
        "도쿄" | "동경" => "Tokyo",
        "오사카" => "Osaka",
        "교토" => "Kyoto",
        "후쿠오카" => "Fukuoka",
        "삿포로" => "Sapporo",
        "나고야" => "Nagoya",
        "오키나와" | "나하" => "Naha",
        "시드니" => "Sydney",
        "멜버른" | "멜번" => "Melbourne",
        "브리즈번" => "Brisbane",
        "퍼스" => "Perth",
        "오클랜드" => "Auckland",
        "웰링턴" => "Wellington",
        "토론토" => "Toronto",
        "밴쿠버" => "Vancouver",
        "몬트리올" => "Montreal",
        "멕시코시티" => "Mexico City",
        "칸쿤" => "Cancun",
        "리마" => "Lima",
        "보고타" => "Bogota",
        "산티아고" => "Santiago",
        "상파울루" | "상파울로" => "Sao Paulo",
        "리우데자네이루" => "Rio de Janeiro",
        "부에노스아이레스" => "Buenos Aires",
        "멘도사" => "Mendoza",
        "코르도바" => "Cordoba",
        "우수아이아" => "Ushuaia",
        "미국" => "Washington DC",
        "캐나다" => "Ottawa",
        "영국" => "London",
        "프랑스" => "Paris",
        "독일" => "Berlin",
        "이탈리아" => "Rome",
        "스페인" => "Madrid",
        "포르투갈" => "Lisbon",
        "네덜란드" => "Amsterdam",
        "벨기에" => "Brussels",
        "스위스" => "Bern",
        "오스트리아" => "Vienna",
        "체코" => "Prague",
        "헝가리" => "Budapest",
        "폴란드" => "Warsaw",
        "덴마크" => "Copenhagen",
        "스웨덴" => "Stockholm",
        "노르웨이" => "Oslo",
        "핀란드" => "Helsinki",
        "아일랜드" => "Dublin",
        "러시아" => "Moscow",
        "튀르키예" | "터키" => "Ankara",
        "일본" => "Tokyo",
        "중국" => "Beijing",
        "대만" => "Taipei",
        "태국" => "Bangkok",
        "베트남" => "Hanoi",
        "말레이시아" => "Kuala Lumpur",
        "인도네시아" => "Jakarta",
        "필리핀" => "Manila",
        "호주" | "오스트레일리아" => "Canberra",
        "뉴질랜드" => "Wellington",
        "멕시코" => "Mexico City",
        "브라질" => "Brasilia",
        "아르헨티나" => "Buenos Aires",
        "칠레" => "Santiago",
        "페루" => "Lima",
        "콜롬비아" => "Bogota",
        "남아공" | "남아프리카공화국" => "Pretoria",
        "이집트" => "Cairo",
        "케냐" => "Nairobi",
        "아랍에미리트" | "아랍에미리트연합" => "Abu Dhabi",
        "사우디아라비아" => "Riyadh",
        _ => location,
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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_memory::{ChatEntry, ChatRole};

    #[test]
    fn extracts_weather_locations() {
        assert_eq!(weather_location_query("Seoul current weather"), "Seoul");
        assert_eq!(weather_location_query("weather in Seoul"), "Seoul");
        assert_eq!(
            weather_location_query("what's the weather today in seoul?"),
            "seoul"
        );
        assert_eq!(
            weather_location_query("What's the weather in Seoul today?"),
            "Seoul"
        );
        assert_eq!(
            weather_location_query("weather in The Hague today"),
            "The Hague"
        );
        assert_eq!(weather_location_query("오늘 서울날씨는?"), "Seoul");
        assert_eq!(weather_location_query("부산은?"), "Busan");
        assert_eq!(weather_location_query("그럼 부산은?"), "Busan");
        assert_eq!(weather_location_query("what about Busan?"), "Busan");
        assert_eq!(weather_location_query("치앙마이 날씨는?"), "치앙마이");
        assert_eq!(weather_location_query("오늘 날씨는?"), "Seoul");
        assert_eq!(weather_location_query("how's the weather today?"), "Seoul");
        assert_eq!(
            weather_location_query("서울이 아니라... 난 지금 퀸즈야"),
            "Queens"
        );
        assert_eq!(weather_location_query("I'm in Queens, not Seoul"), "Queens");
        assert_eq!(
            weather_location_query("What is Seoul's current weather?"),
            "Seoul"
        );
        assert_eq!(weather_location_query("서울 현재 날씨"), "Seoul");
    }

    #[test]
    fn resolves_pronoun_weather_locations_from_recent_place_context() {
        let history = vec![
            ChatEntry::new(ChatRole::User, "okay. please let me know staten island"),
            ChatEntry::new(
                ChatRole::Assistant,
                "Staten Island is one of the five boroughs of New York City.",
            ),
        ];

        assert_eq!(
            weather_location_query_for_chat(&history, "how's the weather of it?"),
            "Staten Island"
        );
        assert_eq!(
            weather_location_query_for_chat(&history, "weather there?"),
            "Staten Island"
        );
    }

    #[test]
    fn maps_common_korean_weather_locations_to_open_meteo_names() {
        assert_eq!(weather_location_query("인천 날씨"), "Incheon");
        assert_eq!(weather_location_query("대전은?"), "Daejeon");
        assert_eq!(weather_location_query("제주시 날씨"), "Jeju City");
        assert_eq!(
            weather_location_query("부에노스아이레스 날씨는?"),
            "Buenos Aires"
        );
        assert_eq!(weather_location_query("뉴욕은?"), "New York");
        assert_eq!(weather_location_query("퀸즈는?"), "Queens");
        assert_eq!(weather_location_query("아르헨티나 날씨는?"), "Buenos Aires");
    }
}
