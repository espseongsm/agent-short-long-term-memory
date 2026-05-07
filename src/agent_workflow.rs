mod actions;
mod prompts;
mod summary;
mod weather_locations;
mod weather_lookup;

#[allow(unused_imports)]
pub use actions::{
    AutomaticActions, automatic_actions, automatic_actions_for_chat, should_auto_amplify,
    should_auto_summarize, should_auto_weather, should_auto_web_search,
};
#[allow(unused_imports)]
pub use prompts::{
    automatic_chat_prompt, prompt_with_amplification, prompt_with_long_term_context,
    prompt_with_weather_context, prompt_with_web_search_context,
};
#[allow(unused_imports)]
pub use summary::{summarize_chat_history, summarize_chat_history_with_usage, summary_prompt};
pub use weather_locations::{weather_location_query, weather_location_query_for_chat};
#[allow(unused_imports)]
pub use weather_lookup::{
    WeatherLookup, clean_translated_weather_location, current_weather_with_translation_fallback,
    translate_weather_location_to_english,
};
