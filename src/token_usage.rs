use std::collections::BTreeMap;

use redis::Commands;
use serde::{Deserialize, Serialize};

use crate::{MemoryError, ShortTermMemory, unix_timestamp_seconds};

const SECONDS_PER_DAY: u64 = 86_400;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenUsageSource {
    FinalAnswer,
    RequestAmplifier,
    SummaryAgent,
    WeatherLocationNormalizer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenUsageRecord {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub timestamp: u64,
    pub source: TokenUsageSource,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DailyTokenUsage {
    pub date: String,
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub sessions: Vec<SessionTokenUsage>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionTokenUsage {
    pub session_id: String,
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl TokenUsageRecord {
    pub fn for_session(
        user_id: &str,
        session_id: &str,
        source: TokenUsageSource,
        tokens: TokenUsageTotals,
    ) -> Self {
        Self::with_metadata(
            user_id,
            session_id,
            unix_timestamp_seconds(),
            source,
            tokens,
        )
    }

    pub fn with_metadata(
        user_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: u64,
        source: TokenUsageSource,
        tokens: TokenUsageTotals,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            session_id: session_id.into(),
            timestamp,
            source,
            input_tokens: tokens.input_tokens,
            output_tokens: tokens.output_tokens,
            total_tokens: tokens.total_tokens,
            cached_input_tokens: tokens.cached_input_tokens,
            cache_creation_input_tokens: tokens.cache_creation_input_tokens,
        }
    }

    pub fn has_usage(&self) -> bool {
        self.input_tokens > 0 || self.output_tokens > 0 || self.total_tokens > 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenUsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl ShortTermMemory {
    pub fn append_token_usage(
        &mut self,
        session: &str,
        record: TokenUsageRecord,
        ttl_seconds: u64,
    ) -> Result<(), MemoryError> {
        if ttl_seconds == 0 {
            return Err(MemoryError::InvalidInput(
                "ttl_seconds must be greater than zero",
            ));
        }
        if !record.has_usage() {
            return Ok(());
        }

        let key = self.token_usage_key(session);
        let value = serde_json::to_string(&record)?;
        let ttl_seconds = i64::try_from(ttl_seconds)
            .map_err(|_| MemoryError::InvalidInput("ttl_seconds is too large"))?;

        let _: usize = self.connection.rpush(&key, value)?;
        let _: bool = self.connection.expire(&key, ttl_seconds)?;

        Ok(())
    }

    pub fn daily_token_usage(&mut self) -> Result<Vec<DailyTokenUsage>, MemoryError> {
        let records = self
            .token_usage_keys()?
            .into_iter()
            .map(|key| {
                let values: Vec<String> = self.connection.lrange(&key, 0, -1)?;
                values
                    .into_iter()
                    .map(|value| serde_json::from_str(&value).map_err(MemoryError::from))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, MemoryError>>()?
            .into_iter()
            .flatten()
            .collect();

        Ok(aggregate_daily_token_usage(records))
    }

    fn token_usage_key(&self, session: &str) -> String {
        self.namespaced_key(&format!("token_usage:{session}"))
    }

    fn token_usage_key_prefix(&self) -> String {
        self.namespaced_key("token_usage:")
    }

    fn token_usage_pattern(&self) -> String {
        format!("{}*", self.token_usage_key_prefix())
    }

    fn token_usage_keys(&mut self) -> Result<Vec<String>, MemoryError> {
        let pattern = self.token_usage_pattern();

        self.keys_matching(&pattern)
    }
}

fn aggregate_daily_token_usage(records: Vec<TokenUsageRecord>) -> Vec<DailyTokenUsage> {
    let mut by_day = BTreeMap::<String, DailyTokenUsage>::new();

    for record in records.into_iter().filter(TokenUsageRecord::has_usage) {
        let date = utc_date(record.timestamp);
        let usage = by_day
            .entry(date.clone())
            .or_insert_with(|| DailyTokenUsage {
                date,
                ..DailyTokenUsage::default()
            });
        usage.calls += 1;
        usage.input_tokens += record.input_tokens;
        usage.output_tokens += record.output_tokens;
        usage.total_tokens += record.total_tokens;
        usage.cached_input_tokens += record.cached_input_tokens;
        usage.cache_creation_input_tokens += record.cache_creation_input_tokens;
        add_session_token_usage(&mut usage.sessions, record);
    }

    for usage in by_day.values_mut() {
        usage.sessions.sort_by(|left, right| {
            right
                .total_tokens
                .cmp(&left.total_tokens)
                .then_with(|| right.calls.cmp(&left.calls))
                .then_with(|| left.session_id.cmp(&right.session_id))
        });
    }

    by_day.into_values().rev().collect()
}

fn add_session_token_usage(sessions: &mut Vec<SessionTokenUsage>, record: TokenUsageRecord) {
    let session_id = if record.session_id.is_empty() {
        "unknown".to_string()
    } else {
        record.session_id
    };

    let usage = match sessions
        .iter_mut()
        .find(|usage| usage.session_id == session_id)
    {
        Some(usage) => usage,
        None => {
            sessions.push(SessionTokenUsage {
                session_id,
                ..SessionTokenUsage::default()
            });
            sessions.last_mut().expect("just pushed session usage")
        }
    };

    usage.calls += 1;
    usage.input_tokens += record.input_tokens;
    usage.output_tokens += record.output_tokens;
    usage.total_tokens += record.total_tokens;
    usage.cached_input_tokens += record.cached_input_tokens;
    usage.cache_creation_input_tokens += record.cache_creation_input_tokens;
}

pub fn utc_date(timestamp: u64) -> String {
    let days = (timestamp / SECONDS_PER_DAY) as i64;
    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn totals(input: u64, output: u64, total: u64) -> TokenUsageTotals {
        TokenUsageTotals {
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            cached_input_tokens: 1,
            cache_creation_input_tokens: 2,
        }
    }

    #[test]
    fn token_usage_records_round_trip_json() {
        let record = TokenUsageRecord::with_metadata(
            "soonmo",
            "work",
            1_799_712_000,
            TokenUsageSource::FinalAnswer,
            totals(10, 5, 15),
        );

        let json = serde_json::to_string(&record).unwrap();
        let parsed: TokenUsageRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, record);
    }

    #[test]
    fn daily_token_usage_groups_by_utc_date() {
        let records = vec![
            TokenUsageRecord::with_metadata(
                "soonmo",
                "work",
                0,
                TokenUsageSource::FinalAnswer,
                totals(10, 5, 15),
            ),
            TokenUsageRecord::with_metadata(
                "soonmo",
                "work",
                3_600,
                TokenUsageSource::RequestAmplifier,
                totals(7, 4, 11),
            ),
            TokenUsageRecord::with_metadata(
                "soonmo",
                "work",
                SECONDS_PER_DAY,
                TokenUsageSource::SummaryAgent,
                totals(1, 2, 3),
            ),
        ];

        let usage = aggregate_daily_token_usage(records);

        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].date, "1970-01-02");
        assert_eq!(usage[0].calls, 1);
        assert_eq!(usage[1].date, "1970-01-01");
        assert_eq!(usage[1].calls, 2);
        assert_eq!(usage[1].input_tokens, 17);
        assert_eq!(usage[1].output_tokens, 9);
        assert_eq!(usage[1].total_tokens, 26);
        assert_eq!(usage[1].cached_input_tokens, 2);
        assert_eq!(usage[1].cache_creation_input_tokens, 4);
        assert_eq!(usage[1].sessions.len(), 1);
        assert_eq!(usage[1].sessions[0].session_id, "work");
        assert_eq!(usage[1].sessions[0].calls, 2);
        assert_eq!(usage[1].sessions[0].total_tokens, 26);
    }

    #[test]
    fn daily_token_usage_ignores_zero_usage() {
        let records = vec![TokenUsageRecord::with_metadata(
            "soonmo",
            "work",
            0,
            TokenUsageSource::WeatherLocationNormalizer,
            TokenUsageTotals::default(),
        )];

        assert!(aggregate_daily_token_usage(records).is_empty());
    }
}
