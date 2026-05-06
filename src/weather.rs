use std::time::Duration;

use anyhow::{Context, Result, ensure};
use reqwest::StatusCode;
use serde::Deserialize;

const DEFAULT_GEOCODING_URL: &str = "http://geocoding-api.open-meteo.com/v1/search";
const DEFAULT_FORECAST_URL: &str = "http://api.open-meteo.com/v1/forecast";
const DEFAULT_WEATHER_TIMEOUT_SECONDS: u64 = 10;

#[derive(Clone)]
pub struct WeatherClient {
    client: reqwest::Client,
    geocoding_url: String,
    forecast_url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeatherReport {
    location: String,
    timezone: String,
    time: String,
    temperature_2m: f64,
    apparent_temperature: f64,
    relative_humidity_2m: u64,
    precipitation: f64,
    weather_code: u64,
    wind_speed_10m: f64,
    wind_direction_10m: u64,
    wind_gusts_10m: f64,
    units: WeatherUnits,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<GeocodingResult>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    country: String,
    #[serde(default)]
    admin1: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ForecastResponse {
    timezone: String,
    current: CurrentWeather,
    current_units: WeatherUnits,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct CurrentWeather {
    time: String,
    temperature_2m: f64,
    apparent_temperature: f64,
    relative_humidity_2m: u64,
    precipitation: f64,
    weather_code: u64,
    wind_speed_10m: f64,
    wind_direction_10m: u64,
    wind_gusts_10m: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct WeatherUnits {
    #[serde(default)]
    temperature_2m: String,
    #[serde(default)]
    apparent_temperature: String,
    #[serde(default)]
    relative_humidity_2m: String,
    #[serde(default)]
    precipitation: String,
    #[serde(default)]
    wind_speed_10m: String,
    #[serde(default)]
    wind_direction_10m: String,
    #[serde(default)]
    wind_gusts_10m: String,
}

impl WeatherClient {
    pub fn from_env() -> Result<Self> {
        let geocoding_url = std::env::var("WEATHER_GEOCODING_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_GEOCODING_URL.to_string());
        let forecast_url = std::env::var("WEATHER_FORECAST_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_FORECAST_URL.to_string());
        let client = reqwest::Client::builder()
            .timeout(weather_timeout())
            .build()
            .context("failed to build weather HTTP client")?;

        Ok(Self {
            client,
            geocoding_url,
            forecast_url,
        })
    }

    pub async fn current_weather(&self, location: &str) -> Result<WeatherReport> {
        let location = location.trim();
        ensure!(!location.is_empty(), "weather location cannot be empty");

        let geocoding_response = self
            .client
            .get(&self.geocoding_url)
            .query(&[
                ("name", location),
                ("count", "1"),
                ("language", "en"),
                ("format", "json"),
            ])
            .send()
            .await
            .with_context(|| format!("failed to geocode weather location `{location}`"))?;
        ensure_success_status(geocoding_response.status(), "weather geocoding")?;
        let geocoding_response = geocoding_response
            .json::<GeocodingResponse>()
            .await
            .context("failed to parse weather geocoding response")?;
        let location = geocoding_response
            .results
            .into_iter()
            .next()
            .context("weather location was not found")?;

        let forecast_response = self
            .client
            .get(&self.forecast_url)
            .query(&[
                ("latitude", location.latitude.to_string()),
                ("longitude", location.longitude.to_string()),
                (
                    "current",
                    "temperature_2m,relative_humidity_2m,apparent_temperature,precipitation,weather_code,wind_speed_10m,wind_direction_10m,wind_gusts_10m".to_string(),
                ),
                ("timezone", "auto".to_string()),
            ])
            .send()
            .await
            .with_context(|| format!("failed to request weather forecast for `{}`", location.name))?;
        ensure_success_status(forecast_response.status(), "weather forecast")?;
        let forecast_response = forecast_response
            .json::<ForecastResponse>()
            .await
            .context("failed to parse weather forecast response")?;

        Ok(WeatherReport::from_responses(location, forecast_response))
    }
}

impl WeatherReport {
    fn from_responses(location: GeocodingResult, forecast: ForecastResponse) -> Self {
        Self {
            location: location_label(&location),
            timezone: forecast.timezone,
            time: forecast.current.time,
            temperature_2m: forecast.current.temperature_2m,
            apparent_temperature: forecast.current.apparent_temperature,
            relative_humidity_2m: forecast.current.relative_humidity_2m,
            precipitation: forecast.current.precipitation,
            weather_code: forecast.current.weather_code,
            wind_speed_10m: forecast.current.wind_speed_10m,
            wind_direction_10m: forecast.current.wind_direction_10m,
            wind_gusts_10m: forecast.current.wind_gusts_10m,
            units: forecast.current_units,
        }
    }

    pub fn to_markdown(&self) -> String {
        format!(
            "Current weather for {} ({}) at {}:\n\n- Conditions: {} (WMO {})\n- Temperature: {:.1} {} (feels like {:.1} {})\n- Humidity: {}{}\n- Precipitation: {:.2} {}\n- Wind: {:.1} {} from {}{}, gusts {:.1} {}\n\nSource: Open-Meteo forecast API.",
            self.location,
            self.timezone,
            self.time,
            weather_code_label(self.weather_code),
            self.weather_code,
            self.temperature_2m,
            fallback_unit(&self.units.temperature_2m, "C"),
            self.apparent_temperature,
            fallback_unit(&self.units.apparent_temperature, "C"),
            self.relative_humidity_2m,
            fallback_unit(&self.units.relative_humidity_2m, "%"),
            self.precipitation,
            fallback_unit(&self.units.precipitation, "mm"),
            self.wind_speed_10m,
            fallback_unit(&self.units.wind_speed_10m, "km/h"),
            self.wind_direction_10m,
            fallback_unit(&self.units.wind_direction_10m, "deg"),
            self.wind_gusts_10m,
            fallback_unit(&self.units.wind_gusts_10m, "km/h"),
        )
    }
}

fn location_label(location: &GeocodingResult) -> String {
    let mut parts = vec![location.name.as_str()];

    if !location.admin1.is_empty() && location.admin1 != location.name {
        parts.push(location.admin1.as_str());
    }

    if !location.country.is_empty() {
        parts.push(location.country.as_str());
    }

    parts.join(", ")
}

fn fallback_unit<'a>(unit: &'a str, fallback: &'a str) -> &'a str {
    if unit.is_empty() { fallback } else { unit }
}

fn weather_code_label(code: u64) -> &'static str {
    match code {
        0 => "clear sky",
        1..=3 => "mainly clear, partly cloudy, or overcast",
        45 | 48 => "fog",
        51 | 53 | 55 => "drizzle",
        56 | 57 => "freezing drizzle",
        61 | 63 | 65 => "rain",
        66 | 67 => "freezing rain",
        71 | 73 | 75 => "snowfall",
        77 => "snow grains",
        80..=82 => "rain showers",
        85 | 86 => "snow showers",
        95 => "thunderstorm",
        96 | 99 => "thunderstorm with hail",
        _ => "unknown",
    }
}

fn ensure_success_status(status: StatusCode, operation: &str) -> Result<()> {
    ensure!(
        status.is_success(),
        "{operation} failed with HTTP status {status}"
    );

    Ok(())
}

fn weather_timeout() -> Duration {
    std::env::var("WEATHER_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_WEATHER_TIMEOUT_SECONDS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> WeatherUnits {
        WeatherUnits {
            temperature_2m: "°C".to_string(),
            apparent_temperature: "°C".to_string(),
            relative_humidity_2m: "%".to_string(),
            precipitation: "mm".to_string(),
            wind_speed_10m: "km/h".to_string(),
            wind_direction_10m: "°".to_string(),
            wind_gusts_10m: "km/h".to_string(),
        }
    }

    #[test]
    fn formats_current_weather_report() {
        let report = WeatherReport::from_responses(
            GeocodingResult {
                name: "Seoul".to_string(),
                latitude: 37.566,
                longitude: 126.9784,
                country: "South Korea".to_string(),
                admin1: "Seoul".to_string(),
            },
            ForecastResponse {
                timezone: "Asia/Seoul".to_string(),
                current_units: units(),
                current: CurrentWeather {
                    time: "2026-05-06T19:00".to_string(),
                    temperature_2m: 18.2,
                    apparent_temperature: 17.5,
                    relative_humidity_2m: 59,
                    precipitation: 0.0,
                    weather_code: 0,
                    wind_speed_10m: 4.7,
                    wind_direction_10m: 238,
                    wind_gusts_10m: 29.2,
                },
            },
        );

        let markdown = report.to_markdown();

        assert!(markdown.contains("Current weather for Seoul, South Korea"));
        assert!(markdown.contains("Conditions: clear sky"));
        assert!(markdown.contains("Temperature: 18.2 °C"));
    }

    #[test]
    fn maps_common_weather_codes() {
        assert_eq!(weather_code_label(0), "clear sky");
        assert_eq!(weather_code_label(63), "rain");
        assert_eq!(weather_code_label(999), "unknown");
    }
}
