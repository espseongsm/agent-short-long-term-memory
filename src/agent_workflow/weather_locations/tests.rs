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
        weather_location_query("hello what's the weather today?"),
        "Seoul"
    );
    assert_eq!(
        weather_location_query("good morning how's the weather today?"),
        "Seoul"
    );
    assert_eq!(
        weather_location_query("hi Chiang Mai weather?"),
        "Chiang Mai"
    );
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
