use agent_memory::ChatEntry;

mod amplify;
mod common;
mod memory;
mod summary;
#[cfg(test)]
mod tests;
mod weather;
mod web;

pub use amplify::should_auto_amplify;
pub use memory::should_auto_long_term_memory;
pub use summary::should_auto_summarize;
pub use weather::should_auto_weather;
pub use web::should_auto_web_search;

use weather::{has_recent_weather_context, is_weather_location_follow_up};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AutomaticActions {
    pub amplify: bool,
    pub weather: bool,
    pub web_search: bool,
    pub long_term_memory: bool,
}

pub fn automatic_actions(prompt: &str) -> AutomaticActions {
    if should_auto_summarize(prompt) {
        return AutomaticActions {
            amplify: false,
            weather: false,
            web_search: false,
            long_term_memory: false,
        };
    }

    let weather = should_auto_weather(prompt);
    let long_term_memory_candidate = should_auto_long_term_memory(prompt);
    let web_search = !weather && !long_term_memory_candidate && should_auto_web_search(prompt);
    let long_term_memory = !weather && !web_search && long_term_memory_candidate;

    AutomaticActions {
        amplify: !weather && !web_search && !long_term_memory && should_auto_amplify(prompt),
        weather,
        web_search,
        long_term_memory,
    }
}

pub fn automatic_actions_for_chat(history: &[ChatEntry], prompt: &str) -> AutomaticActions {
    let mut actions = automatic_actions(prompt);

    if !actions.weather
        && has_recent_weather_context(history)
        && is_weather_location_follow_up(prompt)
    {
        actions.amplify = false;
        actions.weather = true;
        actions.web_search = false;
        actions.long_term_memory = false;

        return actions;
    }

    actions
}
