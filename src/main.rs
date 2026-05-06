use std::{
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use agent_memory::{
    ChatEntry, ChatRole, DEFAULT_PROMPT_DIR, ShortTermMemory, save_system_prompt_yaml,
};
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

mod agent_workflow;
mod llm;
mod long_term_memory;
mod tui;
mod weather;
mod web_search;

const DEFAULT_VALKEY_URL: &str = "redis://127.0.0.1:6379/";
const DEFAULT_NAMESPACE: &str = "agent:short-term";
const DEFAULT_HISTORY_LIMIT: usize = 20;
const DEFAULT_CHAT_TTL_SECONDS: u64 = 86_400;
const DEFAULT_OPENAI_MODEL: &str = "Qwen/Qwen3.6-35B-A3B";
const AGENT_PREAMBLE: &str =
    "You are a concise helpful assistant. Use the conversation history when it helps.";
const REQUEST_AMPLIFIER_PREAMBLE: &str = "You are a user request amplifier sub-agent. Rewrite the user's request into a clearer, more complete, implementation-ready request. Preserve the user's intent, surface assumptions, and avoid adding unrelated requirements.";
static RUNTIME_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Parser)]
#[command(about = "Terminal agent with short-term memory in Valkey")]
struct Cli {
    #[arg(long, env = "VALKEY_URL", default_value = DEFAULT_VALKEY_URL)]
    valkey_url: String,

    #[arg(long, default_value = DEFAULT_NAMESPACE)]
    namespace: String,

    #[arg(long, env = "AGENT_USER_ID")]
    user_id: Option<String>,

    #[arg(long, env = "PGVECTOR_URL")]
    pgvector_url: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
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
    WebSearch {
        query: String,

        #[arg(long, default_value_t = web_search::DEFAULT_WEB_SEARCH_LIMIT)]
        limit: usize,
    },
    Weather {
        location: String,
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

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let mut cli = Cli::parse();
    let user_id = cli.user_id.take().unwrap_or_else(|| runtime_id("user"));
    let command = match cli.command.take() {
        Some(command) => command,
        None => default_tui_command()?,
    };

    match &command {
        Command::Tui {
            history_limit,
            ttl_seconds,
            ..
        }
        | Command::Chat {
            history_limit,
            ttl_seconds,
            ..
        } => {
            ensure!(
                *history_limit > 0,
                "history_limit must be greater than zero"
            );
            ensure!(*ttl_seconds > 0, "ttl_seconds must be greater than zero");
        }
        Command::Remember { ttl_seconds, .. } => {
            ensure!(*ttl_seconds > 0, "ttl_seconds must be greater than zero");
        }
        Command::WebSearch { limit, .. } => {
            ensure!(*limit > 0, "limit must be greater than zero");
        }
        Command::LongTermIndex { chunk_chars, .. } => {
            ensure!(*chunk_chars > 0, "chunk_chars must be greater than zero");
        }
        Command::LongTermSearch { limit, .. } => {
            ensure!(*limit > 0, "limit must be greater than zero");
        }
        _ => {}
    }

    if let Command::WebSearch { query, limit } = &command {
        let web_search =
            web_search::WebSearchClient::from_env().context("failed to init web search")?;
        let results = web_search
            .search(query, *limit)
            .await
            .context("failed to run web search")?;

        println!("{}", results.to_markdown());
        return Ok(());
    }

    if let Command::Weather { location } = &command {
        let weather = weather::WeatherClient::from_env().context("failed to init weather")?;
        let report = weather
            .current_weather(location)
            .await
            .context("failed to fetch weather")?;

        println!("{}", report.to_markdown());
        return Ok(());
    }

    if let Command::LongTermIndex { path, chunk_chars } = &command {
        let long_term = connect_long_term_memory(cli.pgvector_url.as_deref()).await?;
        let indexed = long_term
            .index_markdown_path(path, *chunk_chars)
            .await
            .with_context(|| format!("failed to index markdown from {}", path.display()))?;

        println!("indexed {indexed} markdown chunks");
        return Ok(());
    }

    if let Command::LongTermSearch { query, limit } = &command {
        let long_term = connect_long_term_memory(cli.pgvector_url.as_deref()).await?;
        let results = long_term
            .search(query, *limit)
            .await
            .context("failed to search long-term memory")?;

        println!(
            "{}",
            long_term_memory::LongTermSearchResult::to_markdown(&results)
        );
        return Ok(());
    }

    if let Command::Amplify {
        request,
        model,
        reasoning_effort,
    } = &command
    {
        let amplifier =
            llm::LlmClient::from_env(model.clone(), REQUEST_AMPLIFIER_PREAMBLE, *reasoning_effort);
        let amplified = amplifier
            .chat(&[], request)
            .await
            .context("failed to amplify user request")?;

        println!("{amplified}");
        return Ok(());
    }

    let mut memory = ShortTermMemory::connect(&cli.valkey_url, &cli.namespace)
        .with_context(|| format!("failed to connect to Valkey at {}", cli.valkey_url))?;

    match command {
        Command::Tui {
            session,
            model,
            reasoning_effort,
            history_limit,
            ttl_seconds,
        } => {
            let session = session.unwrap_or_else(|| runtime_id("session"));
            save_system_prompt_yaml(DEFAULT_PROMPT_DIR, AGENT_PREAMBLE)
                .context("failed to save system prompt YAML")?;
            tui::run(
                &mut memory,
                tui::TuiConfig {
                    user_id,
                    session,
                    model,
                    reasoning_effort,
                    history_limit,
                    ttl_seconds,
                    preamble: AGENT_PREAMBLE,
                    amplifier_preamble: REQUEST_AMPLIFIER_PREAMBLE,
                    pgvector_url: cli.pgvector_url,
                },
            )
            .await?;
        }
        Command::Chat {
            prompt,
            session,
            model,
            reasoning_effort,
            history_limit,
            ttl_seconds,
        } => {
            let session = session.unwrap_or_else(|| runtime_id("session"));
            let history = memory
                .chat_history(&session)
                .context("failed to read chat history")?;
            save_system_prompt_yaml(DEFAULT_PROMPT_DIR, AGENT_PREAMBLE)
                .context("failed to save system prompt YAML")?;
            let user_entry = ChatEntry::for_session(&user_id, &session, ChatRole::User, prompt);

            let prompt_for_llm = automatic_chat_prompt(
                cli.pgvector_url.as_deref(),
                &history,
                &user_entry.content,
                &model,
                reasoning_effort,
            )
            .await;
            let llm = llm::LlmClient::from_env(model, AGENT_PREAMBLE, reasoning_effort);
            let response = llm
                .chat(&history, &prompt_for_llm)
                .await
                .context("failed to run agent chat")?;

            memory
                .append_chat_entry(&session, user_entry, history_limit, ttl_seconds)
                .context("failed to save user chat history")?;
            memory
                .append_chat_entry(
                    &session,
                    ChatEntry::for_session(&user_id, &session, ChatRole::Assistant, &response),
                    history_limit,
                    ttl_seconds,
                )
                .context("failed to save assistant chat history")?;

            println!("{response}");
        }
        Command::History { session } => {
            let session = session.unwrap_or_else(|| runtime_id("session"));
            let history = memory
                .chat_history(&session)
                .context("failed to read chat history")?;
            print_chat_entries(history);
        }
        Command::Remember {
            key,
            value,
            ttl_seconds,
        } => {
            memory
                .remember(&key, &value, ttl_seconds)
                .context("failed to write short-term memory")?;
            println!("remembered {key} for {ttl_seconds}s");
        }
        Command::Recall { key } => match memory
            .recall(&key)
            .context("failed to read short-term memory")?
        {
            Some(value) => println!("{value}"),
            None => println!("not found"),
        },
        Command::Search { query, session } => {
            let session = session.unwrap_or_else(|| runtime_id("session"));
            let matches = memory
                .search_chat_history(&session, &query)
                .context("failed to search chat history")?;
            print_chat_entries(matches);
        }
        Command::WebSearch { .. } => unreachable!("web search is handled before Valkey connects"),
        Command::Weather { .. } => unreachable!("weather is handled before Valkey connects"),
        Command::LongTermIndex { .. } => {
            unreachable!("long-term indexing is handled before Valkey connects")
        }
        Command::LongTermSearch { .. } => {
            unreachable!("long-term search is handled before Valkey connects")
        }
        Command::Amplify { .. } => unreachable!("amplify is handled before Valkey connects"),
        Command::Forget { key } => {
            let removed = memory
                .forget(&key)
                .context("failed to delete short-term memory")?;
            println!("{}", if removed { "forgotten" } else { "not found" });
        }
    }

    Ok(())
}

fn default_tui_command() -> Result<Command> {
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

async fn automatic_chat_prompt(
    pgvector_url: Option<&str>,
    history: &[ChatEntry],
    prompt: &str,
    model: &str,
    reasoning_effort: Option<llm::ReasoningEffort>,
) -> String {
    let actions = agent_workflow::automatic_actions(prompt);
    let mut prompt_for_llm = prompt.to_string();

    if actions.amplify {
        eprintln!("auto request amplifier: expanding vague request");
        let amplifier = llm::LlmClient::from_env(
            model.to_string(),
            REQUEST_AMPLIFIER_PREAMBLE,
            reasoning_effort,
        );

        match amplifier.chat(history, prompt).await {
            Ok(amplified) => {
                eprintln!("auto request amplifier: produced amplified request");
                prompt_for_llm = agent_workflow::prompt_with_amplification(prompt, &amplified);
            }
            Err(error) => eprintln!("auto request amplifier failed: {error:#}"),
        }
    }

    if actions.weather {
        let location = agent_workflow::weather_location_query(prompt);
        eprintln!("auto weather: fetching current weather for `{location}`");

        match weather::WeatherClient::from_env() {
            Ok(weather) => match weather.current_weather(&location).await {
                Ok(report) => {
                    eprintln!("auto weather: received current weather");
                    prompt_for_llm = agent_workflow::prompt_with_weather_context(
                        &prompt_for_llm,
                        &report.to_markdown(),
                    );
                }
                Err(error) => eprintln!("auto weather failed: {error:#}"),
            },
            Err(error) => eprintln!("auto weather init failed: {error:#}"),
        }
    }

    if actions.web_search {
        eprintln!("auto web search: searching for current context");
        match web_search::WebSearchClient::from_env() {
            Ok(web_search) => {
                match web_search
                    .search(prompt, web_search::DEFAULT_WEB_SEARCH_LIMIT)
                    .await
                {
                    Ok(results) => {
                        eprintln!("auto web search: received results");
                        prompt_for_llm = agent_workflow::prompt_with_web_search_context(
                            &prompt_for_llm,
                            &results.to_markdown(),
                        );
                    }
                    Err(error) => eprintln!("auto web search failed: {error:#}"),
                }
            }
            Err(error) => eprintln!("auto web search init failed: {error:#}"),
        }
    }

    if let Some(long_term_context) = automatic_long_term_context(pgvector_url, prompt).await {
        prompt_for_llm =
            agent_workflow::prompt_with_long_term_context(&prompt_for_llm, &long_term_context);
    }

    prompt_for_llm
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

async fn connect_long_term_memory(
    pgvector_url: Option<&str>,
) -> Result<long_term_memory::LongTermMemory> {
    let pgvector_url = pgvector_url.context("PGVECTOR_URL must be set for long-term memory")?;

    long_term_memory::connect(pgvector_url).await
}

fn print_chat_entries(entries: Vec<ChatEntry>) {
    if entries.is_empty() {
        println!("not found");
        return;
    }

    for entry in entries {
        println!(
            "{}{}: {}",
            chat_entry_metadata(&entry),
            entry.role,
            entry.content
        );
    }
}

fn chat_entry_metadata(entry: &ChatEntry) -> String {
    if entry.user_id.is_empty() && entry.session_id.is_empty() && entry.timestamp == 0 {
        return String::new();
    }

    format!(
        "[user={} session={} timestamp={}] ",
        fallback_metadata(&entry.user_id),
        fallback_metadata(&entry.session_id),
        entry.timestamp
    )
}

fn fallback_metadata(value: &str) -> &str {
    if value.is_empty() { "unknown" } else { value }
}

fn runtime_id(prefix: &str) -> String {
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
}
