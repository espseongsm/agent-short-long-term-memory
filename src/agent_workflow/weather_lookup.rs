use crate::{
    llm::{LlmClient, ReasoningEffort},
    weather::{WeatherClient, WeatherReport},
};
use anyhow::Context;

pub struct WeatherLookup {
    pub report: WeatherReport,
    pub translated_location: Option<String>,
}

pub async fn current_weather_with_translation_fallback(
    weather: &WeatherClient,
    location: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    weather_translator_preamble: &str,
) -> anyhow::Result<WeatherLookup> {
    let direct_error = match weather.current_weather(location).await {
        Ok(report) => {
            return Ok(WeatherLookup {
                report,
                translated_location: None,
            });
        }
        Err(error) => error,
    };

    let translated_location = translate_weather_location_to_english(
        location,
        model,
        reasoning_effort,
        weather_translator_preamble,
    )
    .await
    .context("failed to translate weather location after direct lookup failed")?
    .ok_or(direct_error)?;
    let report = weather
        .current_weather(&translated_location)
        .await
        .with_context(|| format!("translated weather lookup for `{translated_location}` failed"))?;

    Ok(WeatherLookup {
        report,
        translated_location: Some(translated_location),
    })
}

pub async fn translate_weather_location_to_english(
    location: &str,
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    weather_translator_preamble: &str,
) -> anyhow::Result<Option<String>> {
    let location = location.trim();
    if location.is_empty() {
        return Ok(None);
    }

    let translator = LlmClient::from_env(
        model.to_string(),
        weather_translator_preamble,
        reasoning_effort,
    )?;
    let prompt = format!(
        "Location name:\n{location}\n\nReturn only the English place name for weather geocoding."
    );
    let translated = translator.chat(&[], &prompt).await?;
    let translated = clean_translated_weather_location(&translated);

    Ok(translated.filter(|translated| !translated.eq_ignore_ascii_case(location)))
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
