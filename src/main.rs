use agent_memory::{ChatEntry, ChatRole, ShortTermMemory};
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

mod llm;
mod tui;

const DEFAULT_VALKEY_URL: &str = "redis://127.0.0.1:6379/";
const DEFAULT_NAMESPACE: &str = "agent:short-term";
const DEFAULT_CHAT_SESSION: &str = "default";
const DEFAULT_HISTORY_LIMIT: usize = 20;
const DEFAULT_CHAT_TTL_SECONDS: u64 = 86_400;
const DEFAULT_OPENAI_MODEL: &str = "Qwen/Qwen3.6-35B-A3B";
const AGENT_PREAMBLE: &str =
    "You are a concise helpful assistant. Use the conversation history when it helps.";

#[derive(Parser)]
#[command(about = "Terminal agent with short-term memory in Valkey")]
struct Cli {
    #[arg(long, env = "VALKEY_URL", default_value = DEFAULT_VALKEY_URL)]
    valkey_url: String,

    #[arg(long, default_value = DEFAULT_NAMESPACE)]
    namespace: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Tui {
        #[arg(long, default_value = DEFAULT_CHAT_SESSION)]
        session: String,

        #[arg(long, env = "OPENAI_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
        model: String,

        #[arg(long, default_value_t = DEFAULT_HISTORY_LIMIT)]
        history_limit: usize,

        #[arg(long, default_value_t = DEFAULT_CHAT_TTL_SECONDS)]
        ttl_seconds: u64,
    },
    Chat {
        prompt: String,

        #[arg(long, default_value = DEFAULT_CHAT_SESSION)]
        session: String,

        #[arg(long, env = "OPENAI_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
        model: String,

        #[arg(long, default_value_t = DEFAULT_HISTORY_LIMIT)]
        history_limit: usize,

        #[arg(long, default_value_t = DEFAULT_CHAT_TTL_SECONDS)]
        ttl_seconds: u64,
    },
    History {
        #[arg(long, default_value = DEFAULT_CHAT_SESSION)]
        session: String,
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

        #[arg(long, default_value = DEFAULT_CHAT_SESSION)]
        session: String,
    },
    Forget {
        key: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    let command = cli.command.unwrap_or_else(default_tui_command);

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
        _ => {}
    }

    let mut memory = ShortTermMemory::connect(&cli.valkey_url, &cli.namespace)
        .with_context(|| format!("failed to connect to Valkey at {}", cli.valkey_url))?;

    match command {
        Command::Tui {
            session,
            model,
            history_limit,
            ttl_seconds,
        } => {
            tui::run(
                &mut memory,
                tui::TuiConfig {
                    session,
                    model,
                    history_limit,
                    ttl_seconds,
                    preamble: AGENT_PREAMBLE,
                },
            )
            .await?;
        }
        Command::Chat {
            prompt,
            session,
            model,
            history_limit,
            ttl_seconds,
        } => {
            let history = memory
                .chat_history(&session)
                .context("failed to read chat history")?;
            let llm = llm::LlmClient::from_env(model, AGENT_PREAMBLE);
            let response = llm
                .chat(&history, prompt.as_str())
                .await
                .context("failed to run agent chat")?;

            memory
                .append_chat_entry(
                    &session,
                    ChatEntry::new(ChatRole::User, prompt),
                    history_limit,
                    ttl_seconds,
                )
                .context("failed to save user chat history")?;
            memory
                .append_chat_entry(
                    &session,
                    ChatEntry::new(ChatRole::Assistant, &response),
                    history_limit,
                    ttl_seconds,
                )
                .context("failed to save assistant chat history")?;

            println!("{response}");
        }
        Command::History { session } => {
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
            let matches = memory
                .search_chat_history(&session, &query)
                .context("failed to search chat history")?;
            print_chat_entries(matches);
        }
        Command::Forget { key } => {
            let removed = memory
                .forget(&key)
                .context("failed to delete short-term memory")?;
            println!("{}", if removed { "forgotten" } else { "not found" });
        }
    }

    Ok(())
}

fn default_tui_command() -> Command {
    Command::Tui {
        session: DEFAULT_CHAT_SESSION.to_string(),
        model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_OPENAI_MODEL.to_string()),
        history_limit: DEFAULT_HISTORY_LIMIT,
        ttl_seconds: DEFAULT_CHAT_TTL_SECONDS,
    }
}

fn print_chat_entries(entries: Vec<ChatEntry>) {
    if entries.is_empty() {
        println!("not found");
        return;
    }

    for entry in entries {
        println!("{}: {}", entry.role, entry.content);
    }
}
