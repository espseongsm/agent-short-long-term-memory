use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use redis::Commands;
use serde::{Deserialize, Serialize};

mod token_usage;

pub use token_usage::{
    DailyTokenUsage, SessionTokenUsage, TokenUsageRecord, TokenUsageSource, TokenUsageTotals,
    utc_date,
};

pub const DEFAULT_PROMPT_DIR: &str = "prompt";
const SYSTEM_PROMPT_FILE: &str = "system.yaml";

pub struct ShortTermMemory {
    connection: redis::Connection,
    namespace: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatSessionSummary {
    pub session_id: String,
    pub entry_count: usize,
    pub user_ids: Vec<String>,
    pub first_timestamp: u64,
    pub last_timestamp: u64,
    pub last_role: ChatRole,
    pub last_content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatConversation {
    pub summary: ChatSessionSummary,
    pub entries: Vec<ChatEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error(transparent)]
    Redis(#[from] redis::RedisError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    InvalidInput(&'static str),
}

#[derive(Debug, thiserror::Error)]
pub enum PromptArchiveError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    #[error("{0}")]
    InvalidInput(&'static str),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}

impl std::fmt::Display for ChatRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatRole::User => formatter.write_str("user"),
            ChatRole::Assistant => formatter.write_str("assistant"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChatEntry {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub timestamp: u64,
    pub role: ChatRole,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SystemPromptArchiveEntry {
    role: String,
    prompt: String,
}

impl ChatEntry {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self::with_metadata("", "", 0, role, content)
    }

    pub fn for_session(
        user_id: &str,
        session_id: &str,
        role: ChatRole,
        content: impl Into<String>,
    ) -> Self {
        Self::with_metadata(user_id, session_id, unix_timestamp_seconds(), role, content)
    }

    pub fn with_metadata(
        user_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: u64,
        role: ChatRole,
        content: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            session_id: session_id.into(),
            timestamp,
            role,
            content: content.into(),
        }
    }
}

impl ShortTermMemory {
    pub fn connect(url: &str, namespace: &str) -> redis::RedisResult<Self> {
        let client = redis::Client::open(url)?;
        let connection = client.get_connection()?;

        Ok(Self {
            connection,
            namespace: normalize_namespace(namespace),
        })
    }

    pub fn remember(&mut self, key: &str, value: &str, ttl_seconds: u64) -> redis::RedisResult<()> {
        let key = self.namespaced_key(key);
        self.connection.set_ex(key, value, ttl_seconds)
    }

    pub fn recall(&mut self, key: &str) -> redis::RedisResult<Option<String>> {
        let key = self.namespaced_key(key);
        self.connection.get(key)
    }

    pub fn forget(&mut self, key: &str) -> redis::RedisResult<bool> {
        let key = self.namespaced_key(key);
        let removed: usize = self.connection.del(key)?;
        Ok(removed > 0)
    }

    pub fn append_chat_entry(
        &mut self,
        session: &str,
        entry: ChatEntry,
        max_entries: usize,
        ttl_seconds: u64,
    ) -> Result<(), MemoryError> {
        if max_entries == 0 {
            return Err(MemoryError::InvalidInput(
                "max_entries must be greater than zero",
            ));
        }

        let key = self.chat_history_key(session);
        let value = serde_json::to_string(&entry)?;
        let max_entries = isize::try_from(max_entries)
            .map_err(|_| MemoryError::InvalidInput("max_entries is too large"))?;
        let ttl_seconds = i64::try_from(ttl_seconds)
            .map_err(|_| MemoryError::InvalidInput("ttl_seconds is too large"))?;

        let _: usize = self.connection.rpush(&key, value)?;
        let _: () = self.connection.ltrim(&key, -max_entries, -1)?;
        let _: bool = self.connection.expire(&key, ttl_seconds)?;

        Ok(())
    }

    pub fn chat_history(&mut self, session: &str) -> Result<Vec<ChatEntry>, MemoryError> {
        let key = self.chat_history_key(session);
        let values: Vec<String> = self.connection.lrange(key, 0, -1)?;

        values
            .into_iter()
            .map(|value| serde_json::from_str(&value).map_err(MemoryError::from))
            .collect()
    }

    pub fn search_chat_history(
        &mut self,
        session: &str,
        query: &str,
    ) -> Result<Vec<ChatEntry>, MemoryError> {
        Ok(search_entries(self.chat_history(session)?, query))
    }

    pub fn chat_session_summaries(
        &mut self,
        limit: usize,
    ) -> Result<Vec<ChatSessionSummary>, MemoryError> {
        if limit == 0 {
            return Err(MemoryError::InvalidInput("limit must be greater than zero"));
        }

        let mut summaries = self
            .chat_conversations()?
            .into_iter()
            .map(|conversation| conversation.summary)
            .collect::<Vec<_>>();

        summaries.truncate(limit);

        Ok(summaries)
    }

    pub fn chat_conversations(&mut self) -> Result<Vec<ChatConversation>, MemoryError> {
        let chat_key_prefix = self.chat_history_key_prefix();
        let mut conversations = self
            .chat_history_keys()?
            .into_iter()
            .map(|key| {
                let session_id = key
                    .strip_prefix(&chat_key_prefix)
                    .unwrap_or(&key)
                    .to_string();
                (session_id, key)
            })
            .map(|(session_id, key)| {
                let values: Vec<String> = self.connection.lrange(&key, 0, -1)?;
                let entries = values
                    .into_iter()
                    .map(|value| serde_json::from_str(&value).map_err(MemoryError::from))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(chat_session_summary(session_id, &entries)
                    .map(|summary| ChatConversation { summary, entries }))
            })
            .collect::<Result<Vec<_>, MemoryError>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        conversations.sort_by(|left, right| {
            right
                .summary
                .last_timestamp
                .cmp(&left.summary.last_timestamp)
                .then_with(|| right.summary.entry_count.cmp(&left.summary.entry_count))
                .then_with(|| left.summary.session_id.cmp(&right.summary.session_id))
        });

        Ok(conversations)
    }

    pub(crate) fn keys_matching(&mut self, pattern: &str) -> Result<Vec<String>, MemoryError> {
        let mut cursor = 0_u64;
        let mut keys = Vec::new();

        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query(&mut self.connection)?;

            keys.extend(batch);
            cursor = next_cursor;

            if cursor == 0 {
                break;
            }
        }

        Ok(keys)
    }

    fn chat_history_keys(&mut self) -> Result<Vec<String>, MemoryError> {
        let pattern = self.chat_history_pattern();

        self.keys_matching(&pattern)
    }

    pub(crate) fn namespaced_key(&self, key: &str) -> String {
        namespaced_key(&self.namespace, key)
    }

    fn chat_history_key(&self, session: &str) -> String {
        self.namespaced_key(&format!("chat:{session}"))
    }

    fn chat_history_key_prefix(&self) -> String {
        self.namespaced_key("chat:")
    }

    fn chat_history_pattern(&self) -> String {
        format!("{}*", self.chat_history_key_prefix())
    }
}

pub fn save_system_prompt_yaml(
    prompt_dir: impl AsRef<Path>,
    prompt: &str,
) -> Result<PathBuf, PromptArchiveError> {
    if prompt.trim().is_empty() {
        return Err(PromptArchiveError::InvalidInput(
            "system prompt cannot be empty",
        ));
    }

    let prompt_dir = prompt_dir.as_ref();
    fs::create_dir_all(prompt_dir)?;

    let archive_entry = SystemPromptArchiveEntry {
        role: "system".to_string(),
        prompt: prompt.to_string(),
    };
    let yaml = serde_yaml::to_string(&archive_entry)?;
    let path = prompt_dir.join(SYSTEM_PROMPT_FILE);
    fs::write(&path, yaml)?;

    Ok(path)
}

pub fn search_entries(entries: Vec<ChatEntry>, query: &str) -> Vec<ChatEntry> {
    let query = query.to_lowercase();

    entries
        .into_iter()
        .filter(|entry| entry.content.to_lowercase().contains(&query))
        .collect()
}

fn chat_session_summary(session_id: String, entries: &[ChatEntry]) -> Option<ChatSessionSummary> {
    let last_entry = entries.last()?;
    let mut user_ids = BTreeSet::new();
    let mut first_timestamp = u64::MAX;
    let mut last_timestamp = 0;

    for entry in entries {
        if !entry.user_id.is_empty() {
            user_ids.insert(entry.user_id.clone());
        }
        if entry.timestamp > 0 {
            first_timestamp = first_timestamp.min(entry.timestamp);
            last_timestamp = last_timestamp.max(entry.timestamp);
        }
    }

    if first_timestamp == u64::MAX {
        first_timestamp = 0;
    }

    Some(ChatSessionSummary {
        session_id,
        entry_count: entries.len(),
        user_ids: user_ids.into_iter().collect(),
        first_timestamp,
        last_timestamp,
        last_role: last_entry.role,
        last_content: last_entry.content.clone(),
    })
}

fn normalize_namespace(namespace: &str) -> String {
    namespace.trim_end_matches(':').to_string()
}

fn namespaced_key(namespace: &str, key: &str) -> String {
    format!("{}:{}", namespace, key)
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
