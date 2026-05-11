use crate::{
    llm::{LlmClient, ReasoningEffort, TokenUsage},
    long_term_memory,
    weather::WeatherClient,
    web_search::{DEFAULT_WEB_SEARCH_LIMIT, WebSearchClient},
};
use agent_memory::{ChatEntry, TokenUsageSource};

use super::{
    actions::automatic_actions_for_chat, weather_locations::weather_location_query_for_chat,
    weather_lookup::current_weather_with_translation_fallback_with_usage,
};

pub struct AutomaticChatPrompt {
    pub prompt: String,
    pub usage_events: Vec<AutomaticTokenUsage>,
}

pub struct AutomaticTokenUsage {
    pub source: TokenUsageSource,
    pub usage: TokenUsage,
}

pub fn prompt_with_amplification(original_prompt: &str, amplified_prompt: &str) -> String {
    format!(
        "Original user request:\n{original_prompt}\n\nAmplified request:\n{amplified_prompt}\n\nAnswer the amplified request while preserving the original intent."
    )
}

pub fn prompt_with_web_search_context(prompt: &str, web_search_results: &str) -> String {
    format!(
        "User request:\n{prompt}\n\nAutomatic web search context:\n{web_search_results}\n\nUse the web search context when it is relevant. If it is not useful, say so briefly and answer from the best available context."
    )
}

pub fn prompt_with_weather_context(prompt: &str, weather_report: &str) -> String {
    format!(
        "User request:\n{prompt}\n\nAutomatic weather context:\n{weather_report}\n\nUse the weather context to answer the user's weather question."
    )
}

pub fn prompt_with_long_term_context(prompt: &str, long_term_context: &str) -> String {
    format!(
        "User request:\n{prompt}\n\nLong-term memory context from local Markdown files:\n{long_term_context}\n\nUse the long-term memory context when it is relevant. Ignore it if it is not useful."
    )
}

#[allow(dead_code)]
pub async fn automatic_chat_prompt(
    pgvector_url: Option<&str>,
    history: &[ChatEntry],
    prompt: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    amplifier_preamble: &str,
    weather_translator_preamble: &str,
) -> String {
    automatic_chat_prompt_with_usage(
        pgvector_url,
        history,
        prompt,
        model,
        reasoning_effort,
        amplifier_preamble,
        weather_translator_preamble,
    )
    .await
    .prompt
}

pub async fn automatic_chat_prompt_with_usage(
    pgvector_url: Option<&str>,
    history: &[ChatEntry],
    prompt: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    amplifier_preamble: &str,
    weather_translator_preamble: &str,
) -> AutomaticChatPrompt {
    let actions = automatic_actions_for_chat(history, prompt);
    let mut prompt_for_llm = prompt.to_string();
    let mut usage_events = Vec::new();

    if actions.amplify {
        eprintln!("auto request amplifier: expanding vague request");
        match LlmClient::from_env(model.to_string(), amplifier_preamble, reasoning_effort) {
            Ok(amplifier) => match amplifier.chat_with_usage(history, prompt).await {
                Ok(response) => {
                    eprintln!("auto request amplifier: produced amplified request");
                    push_usage_event(
                        &mut usage_events,
                        TokenUsageSource::RequestAmplifier,
                        response.usage,
                    );
                    prompt_for_llm = prompt_with_amplification(prompt, &response.text);
                }
                Err(error) => eprintln!("auto request amplifier failed: {error:#}"),
            },
            Err(error) => {
                eprintln!("auto request amplifier init failed: {error:#}");
            }
        }
    }

    if actions.weather {
        let location = weather_location_query_for_chat(history, prompt);
        eprintln!("auto weather: fetching current weather for `{location}`");

        match WeatherClient::from_env() {
            Ok(weather) => {
                match current_weather_with_translation_fallback_with_usage(
                    &weather,
                    &location,
                    model,
                    reasoning_effort,
                    weather_translator_preamble,
                )
                .await
                {
                    Ok(result) => {
                        if let Some(usage) = result.normalizer_usage {
                            push_usage_event(
                                &mut usage_events,
                                TokenUsageSource::WeatherLocationNormalizer,
                                usage,
                            );
                        }

                        if let Some(translated_location) = &result.lookup.translated_location {
                            eprintln!(
                                "auto weather: received current weather for translated location `{translated_location}`"
                            );
                        } else {
                            eprintln!("auto weather: received current weather");
                        }

                        prompt_for_llm = prompt_with_weather_context(
                            &prompt_for_llm,
                            &result.lookup.report.to_markdown(),
                        );
                    }
                    Err(error) => eprintln!("auto weather failed: {error:#}"),
                }
            }
            Err(error) => eprintln!("auto weather init failed: {error:#}"),
        }
    }

    if actions.web_search {
        eprintln!("auto web search: searching for current context");
        match WebSearchClient::from_env() {
            Ok(web_search) => match web_search.search(prompt, DEFAULT_WEB_SEARCH_LIMIT).await {
                Ok(results) => {
                    eprintln!("auto web search: received results");
                    prompt_for_llm =
                        prompt_with_web_search_context(&prompt_for_llm, &results.to_markdown());
                }
                Err(error) => eprintln!("auto web search failed: {error:#}"),
            },
            Err(error) => eprintln!("auto web search init failed: {error:#}"),
        }
    }

    if actions.long_term_memory
        && let Some(long_term_context) = automatic_long_term_context(pgvector_url, prompt).await
    {
        prompt_for_llm = prompt_with_long_term_context(&prompt_for_llm, &long_term_context);
    }

    AutomaticChatPrompt {
        prompt: prompt_for_llm,
        usage_events,
    }
}

fn push_usage_event(
    events: &mut Vec<AutomaticTokenUsage>,
    source: TokenUsageSource,
    usage: TokenUsage,
) {
    if usage.has_usage() {
        events.push(AutomaticTokenUsage { source, usage });
    }
}

async fn automatic_long_term_context(pgvector_url: Option<&str>, prompt: &str) -> Option<String> {
    let pgvector_url = pgvector_url?;

    eprintln!("auto long-term memory: searching local markdown context");
    let long_term = match long_term_memory::connect(pgvector_url).await {
        Ok(long_term) => long_term,
        Err(error) => {
            eprintln!("auto long-term memory init failed: {error:#}");
            return None;
        }
    };
    let results = match long_term.search(prompt, 3).await {
        Ok(results) => results,
        Err(error) => {
            eprintln!("auto long-term memory search failed: {error:#}");
            return None;
        }
    };

    if results.is_empty() {
        return None;
    }

    eprintln!("auto long-term memory: found context");
    Some(long_term_memory::LongTermSearchResult::to_markdown(
        &results,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_enrichment_preserves_original_request() {
        let prompt = prompt_with_amplification("fix it", "Fix the login button alignment.");

        assert!(prompt.contains("Original user request:\nfix it"));
        assert!(prompt.contains("Amplified request:\nFix the login button alignment."));
    }
}
