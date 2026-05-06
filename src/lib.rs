use redis::Commands;
use serde::{Deserialize, Serialize};

pub struct ShortTermMemory {
    connection: redis::Connection,
    namespace: String,
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
    pub role: ChatRole,
    pub content: String,
}

impl ChatEntry {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
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

    fn namespaced_key(&self, key: &str) -> String {
        namespaced_key(&self.namespace, key)
    }

    fn chat_history_key(&self, session: &str) -> String {
        self.namespaced_key(&format!("chat:{session}"))
    }
}

pub fn search_entries(entries: Vec<ChatEntry>, query: &str) -> Vec<ChatEntry> {
    let query = query.to_lowercase();

    entries
        .into_iter()
        .filter(|entry| entry.content.to_lowercase().contains(&query))
        .collect()
}

fn normalize_namespace(namespace: &str) -> String {
    namespace.trim_end_matches(':').to_string()
}

fn namespaced_key(namespace: &str, key: &str) -> String {
    format!("{}:{}", namespace, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_namespace_before_building_keys() {
        let namespace = normalize_namespace("agent:short-term:");

        assert_eq!(
            namespaced_key(&namespace, "session"),
            "agent:short-term:session"
        );
    }

    #[test]
    fn filters_chat_entries_case_insensitively() {
        let entries = vec![
            ChatEntry::new(ChatRole::User, "Remember the Valkey setup"),
            ChatEntry::new(
                ChatRole::Assistant,
                "The server URL is redis://127.0.0.1:6379/",
            ),
        ];

        assert_eq!(
            search_entries(entries, "valkey"),
            vec![ChatEntry::new(ChatRole::User, "Remember the Valkey setup")]
        );
    }

    #[test]
    #[ignore = "requires a Valkey server at VALKEY_URL or redis://127.0.0.1:6379/"]
    fn remembers_recalls_and_forgets_with_valkey() {
        let url =
            std::env::var("VALKEY_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
        let mut memory = ShortTermMemory::connect(&url, "agent:test").unwrap();

        memory
            .remember("integration", "short lived value", 30)
            .unwrap();

        assert_eq!(
            memory.recall("integration").unwrap(),
            Some("short lived value".to_string())
        );
        assert!(memory.forget("integration").unwrap());
        assert_eq!(memory.recall("integration").unwrap(), None);
    }

    #[test]
    #[ignore = "requires a Valkey server at VALKEY_URL or redis://127.0.0.1:6379/"]
    fn stores_and_searches_chat_history_with_valkey() {
        let url =
            std::env::var("VALKEY_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
        let mut memory = ShortTermMemory::connect(&url, "agent:test").unwrap();

        memory.forget("chat:integration-session").unwrap();
        memory
            .append_chat_entry(
                "integration-session",
                ChatEntry::new(ChatRole::User, "Searchable Valkey message"),
                10,
                30,
            )
            .unwrap();

        assert_eq!(
            memory
                .search_chat_history("integration-session", "valkey")
                .unwrap(),
            vec![ChatEntry::new(ChatRole::User, "Searchable Valkey message")]
        );

        memory.forget("chat:integration-session").unwrap();
    }
}
