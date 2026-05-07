use std::{
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde::Deserialize;

use crate::{llm, long_term_memory, web_search};

const DEFAULT_VALKEY_URL: &str = "redis://127.0.0.1:6379/";
const DEFAULT_NAMESPACE: &str = "agent:short-term";
const DEFAULT_HISTORY_LIMIT: usize = 20;
const DEFAULT_CHAT_TTL_SECONDS: u64 = 86_400;
const DEFAULT_OPENAI_MODEL: &str = "gpt-5.5";

const AGENT_PROMPT_YAML: &str = include_str!("../prompt/agent.yaml");
const REQUEST_AMPLIFIER_PROMPT_YAML: &str = include_str!("../prompt/request_amplifier.yaml");
const SUMMARY_AGENT_PROMPT_YAML: &str = include_str!("../prompt/summary_agent.yaml");
const WEATHER_LOCATION_NORMALIZER_PROMPT_YAML: &str =
    include_str!("../prompt/weather_location_normalizer.yaml");

static RUNTIME_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentPrompts {
    pub(crate) agent: String,
    pub(crate) request_amplifier: String,
    pub(crate) summary_agent: String,
    pub(crate) weather_location_normalizer: String,
}

#[derive(Deserialize)]
struct PromptFile {
    role: String,
    prompt: String,
}

pub(crate) fn load_agent_prompts() -> Result<AgentPrompts> {
    Ok(AgentPrompts {
        agent: parse_prompt_yaml("agent.yaml", AGENT_PROMPT_YAML)?,
        request_amplifier: parse_prompt_yaml(
            "request_amplifier.yaml",
            REQUEST_AMPLIFIER_PROMPT_YAML,
        )?,
        summary_agent: parse_prompt_yaml("summary_agent.yaml", SUMMARY_AGENT_PROMPT_YAML)?,
        weather_location_normalizer: parse_prompt_yaml(
            "weather_location_normalizer.yaml",
            WEATHER_LOCATION_NORMALIZER_PROMPT_YAML,
        )?,
    })
}

fn parse_prompt_yaml(name: &str, yaml: &str) -> Result<String> {
    let prompt_file: PromptFile =
        serde_yaml::from_str(yaml).with_context(|| format!("failed to parse prompt/{name}"))?;

    ensure!(
        prompt_file.role == "system",
        "prompt/{name} role must be system"
    );
    ensure!(
        !prompt_file.prompt.trim().is_empty(),
        "prompt/{name} prompt cannot be empty"
    );

    Ok(prompt_file.prompt)
}

#[derive(Parser)]
#[command(about = "Terminal agent with short-term memory in Valkey")]
pub(crate) struct Cli {
    #[arg(long, env = "VALKEY_URL", default_value = DEFAULT_VALKEY_URL)]
    pub(crate) valkey_url: String,

    #[arg(long, default_value = DEFAULT_NAMESPACE)]
    pub(crate) namespace: String,

    #[arg(long, env = "AGENT_USER_ID")]
    pub(crate) user_id: Option<String>,

    #[arg(long, env = "PGVECTOR_URL")]
    pub(crate) pgvector_url: Option<String>,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    Tui {
        #[arg(long)]
        session: Option<String>,

        #[arg(long, env = "OPENAI_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
        model: String,

        #[arg(long, env = "LLM_REASONING_EFFORT", value_enum)]
        reasoning_effort: Option<llm::ReasoningEffort>,

        #[arg(long, default_value_t = DEFAULT_HISTORY_LIMIT)]
        history_limit: usize,

        #[arg(long, default_value_t = DEFAULT_CHAT_TTL_SECONDS)]
        ttl_seconds: u64,
    },
    Chat {
        prompt: String,

        #[arg(long)]
        session: Option<String>,

        #[arg(long, env = "OPENAI_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
        model: String,

        #[arg(long, env = "LLM_REASONING_EFFORT", value_enum)]
        reasoning_effort: Option<llm::ReasoningEffort>,

        #[arg(long, default_value_t = DEFAULT_HISTORY_LIMIT)]
        history_limit: usize,

        #[arg(long, default_value_t = DEFAULT_CHAT_TTL_SECONDS)]
        ttl_seconds: u64,
    },
    History {
        #[arg(long)]
        session: Option<String>,
    },
    Remember {
        key: String,
        value: String,

        #[arg(long, default_value_t = 3600)]
        ttl_seconds: u64,
    },
    Recall {
        key: String,
    },
    Search {
        query: String,

        #[arg(long)]
        session: Option<String>,
    },
    Summary {
        #[arg(long)]
        session: Option<String>,

        #[arg(long, env = "OPENAI_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
        model: String,

        #[arg(long, env = "LLM_REASONING_EFFORT", value_enum)]
        reasoning_effort: Option<llm::ReasoningEffort>,

        #[arg(long, default_value_t = DEFAULT_HISTORY_LIMIT)]
        history_limit: usize,
    },
    WebSearch {
        query: String,

        #[arg(long, default_value_t = web_search::DEFAULT_WEB_SEARCH_LIMIT)]
        limit: usize,
    },
    Weather {
        location: String,

        #[arg(long, env = "OPENAI_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
        model: String,

        #[arg(long, env = "LLM_REASONING_EFFORT", value_enum)]
        reasoning_effort: Option<llm::ReasoningEffort>,
    },
    LongTermIndex {
        path: PathBuf,

        #[arg(long, default_value_t = long_term_memory::default_chunk_chars())]
        chunk_chars: usize,
    },
    LongTermSearch {
        query: String,

        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    Amplify {
        request: String,

        #[arg(long, env = "OPENAI_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
        model: String,

        #[arg(long, env = "LLM_REASONING_EFFORT", value_enum)]
        reasoning_effort: Option<llm::ReasoningEffort>,
    },
    Forget {
        key: String,
    },
}

pub(crate) fn default_tui_command() -> Result<Command> {
    default_tui_command_from_values(
        std::env::var("OPENAI_MODEL").ok(),
        std::env::var("LLM_REASONING_EFFORT").ok(),
    )
}

fn default_tui_command_from_values(
    model: Option<String>,
    reasoning_effort: Option<String>,
) -> Result<Command> {
    Ok(Command::Tui {
        session: None,
        model: model.unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string()),
        reasoning_effort: llm::ReasoningEffort::from_env_value(reasoning_effort.as_deref())?,
        history_limit: DEFAULT_HISTORY_LIMIT,
        ttl_seconds: DEFAULT_CHAT_TTL_SECONDS,
    })
}

pub(crate) fn runtime_id(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = RUNTIME_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{prefix}-{timestamp}-{}-{counter}", process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_ids_use_prefix_and_renew() {
        let first = runtime_id("session");
        let second = runtime_id("session");

        assert!(first.starts_with("session-"));
        assert!(second.starts_with("session-"));
        assert_ne!(first, second);
    }

    #[test]
    fn default_tui_command_uses_runtime_session_resolution() {
        match default_tui_command_from_values(None, None).unwrap() {
            Command::Tui {
                session,
                reasoning_effort,
                ..
            } => {
                assert_eq!(session, None);
                assert_eq!(reasoning_effort, None);
            }
            _ => panic!("default command should open the TUI"),
        }
    }

    #[test]
    fn default_tui_command_reads_reasoning_effort_from_env_value() {
        match default_tui_command_from_values(None, Some("low".to_string())).unwrap() {
            Command::Tui {
                reasoning_effort, ..
            } => assert_eq!(reasoning_effort, Some(llm::ReasoningEffort::Low)),
            _ => panic!("default command should open the TUI"),
        }
    }

    #[test]
    fn prompt_yaml_files_load_prompt_text() {
        let prompts = load_agent_prompts().unwrap();

        assert!(prompts.agent.contains("concise helpful assistant"));
        assert!(prompts.request_amplifier.contains("request amplifier"));
        assert!(prompts.summary_agent.contains("summary sub-agent"));
        assert!(
            prompts
                .weather_location_normalizer
                .contains("weather location normalizer")
        );
        assert!(!prompts.agent.contains("role: system"));
    }
}
