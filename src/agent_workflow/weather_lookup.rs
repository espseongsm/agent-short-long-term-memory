use crate::{
    llm::{LlmClient, ReasoningEffort, TokenUsage},
    weather::{WeatherClient, WeatherReport},
};
use anyhow::Context;

pub struct WeatherLookup {
    pub report: WeatherReport,
    pub translated_location: Option<String>,
}

pub struct WeatherLookupWithUsage {
    pub lookup: WeatherLookup,
    pub normalizer_usage: Option<TokenUsage>,
}

pub struct WeatherLocationTranslation {
    pub location: Option<String>,
    pub usage: TokenUsage,
}

pub async fn current_weather_with_translation_fallback(
    weather: &WeatherClient,
    location: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    weather_translator_preamble: &str,
) -> anyhow::Result<WeatherLookup> {
    Ok(current_weather_with_translation_fallback_with_usage(
        weather,
        location,
        model,
        reasoning_effort,
        weather_translator_preamble,
    )
    .await?
    .lookup)
}

pub async fn current_weather_with_translation_fallback_with_usage(
    weather: &WeatherClient,
    location: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    weather_translator_preamble: &str,
) -> anyhow::Result<WeatherLookupWithUsage> {
    let direct_error = match weather.current_weather(location).await {
        Ok(report) => {
            return Ok(WeatherLookupWithUsage {
                lookup: WeatherLookup {
                    report,
                    translated_location: None,
                },
                normalizer_usage: None,
            });
        }
        Err(error) => error,
    };

    let translation = translate_weather_location_to_english_with_usage(
        location,
        model,
        reasoning_effort,
        weather_translator_preamble,
    )
    .await
    .context("failed to translate weather location after direct lookup failed")?;
    let translated_location = translation.location.ok_or(direct_error)?;
    let report = weather
        .current_weather(&translated_location)
        .await
        .with_context(|| format!("translated weather lookup for `{translated_location}` failed"))?;

    Ok(WeatherLookupWithUsage {
        lookup: WeatherLookup {
            report,
            translated_location: Some(translated_location),
        },
        normalizer_usage: translation.usage.has_usage().then_some(translation.usage),
    })
}

#[allow(dead_code)]
pub async fn translate_weather_location_to_english(
    location: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    weather_translator_preamble: &str,
) -> anyhow::Result<Option<String>> {
    Ok(translate_weather_location_to_english_with_usage(
        location,
        model,
        reasoning_effort,
        weather_translator_preamble,
    )
    .await?
    .location)
}

pub async fn translate_weather_location_to_english_with_usage(
    location: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    weather_translator_preamble: &str,
) -> anyhow::Result<WeatherLocationTranslation> {
    let location = location.trim();
    if location.is_empty() {
        return Ok(WeatherLocationTranslation {
            location: None,
            usage: TokenUsage::default(),
        });
    }

    let translator = LlmClient::from_env(
        model.to_string(),
        weather_translator_preamble,
        reasoning_effort,
    )?;
    let prompt = format!(
        "Location name:\n{location}\n\nReturn only the English place name for weather geocoding."
    );
    let response = translator.chat_with_usage(&[], &prompt).await?;
    let translated = clean_translated_weather_location(&response.text);

    Ok(WeatherLocationTranslation {
        location: translated.filter(|translated| !translated.eq_ignore_ascii_case(location)),
        usage: response.usage,
    })
}

pub fn clean_translated_weather_location(raw: &str) -> Option<String> {
    let location = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '"' | '\'' | '`' | '“' | '”' | '‘' | '’' | '.' | '?' | '!' | ':' | ';'
                )
        })
        .trim();

    if location.is_empty()
        || location.chars().count() > 80
        || looks_like_explained_translation(location)
    {
        return None;
    }

    Some(location.to_string())
}

fn looks_like_explained_translation(location: &str) -> bool {
    let lower = location.to_lowercase();

    lower.contains(" is ")
        || lower.contains(" means ")
        || lower.contains(" translates ")
        || lower.contains(" translation ")
        || lower.contains(" -> ")
        || lower.contains(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_translated_weather_locations() {
        assert_eq!(
            clean_translated_weather_location("Chiang Mai\n"),
            Some("Chiang Mai".to_string())
        );
        assert_eq!(
            clean_translated_weather_location("`Chiang Mai`."),
            Some("Chiang Mai".to_string())
        );
    }

    #[test]
    fn rejects_invalid_translated_weather_locations() {
        assert_eq!(clean_translated_weather_location(""), None);
        assert_eq!(
            clean_translated_weather_location("The English place name is Chiang Mai."),
            None
        );
        assert_eq!(clean_translated_weather_location(&"a".repeat(81)), None);
    }
}
