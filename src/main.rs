use agent_memory::{
    ChatEntry, ChatRole, DEFAULT_PROMPT_DIR, ShortTermMemory, TokenUsageSource,
    save_system_prompt_yaml,
};
use anyhow::{Context, Result, ensure};
use clap::Parser;

mod agent_workflow;
mod cli;
mod dashboard;
mod llm;
mod long_term_memory;
mod tui;
mod usage;
mod weather;
mod web_search;

use cli::{Cli, Command, default_tui_command, load_agent_prompts, runtime_id};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let mut cli = Cli::parse();
    let user_id = cli.user_id.take().unwrap_or_else(|| runtime_id("user"));
    let command = match cli.command.take() {
        Some(command) => command,
        None => default_tui_command()?,
    };
    let prompts = load_agent_prompts().context("failed to load agent prompts")?;

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
        Command::Summary { history_limit, .. } => {
            ensure!(
                *history_limit > 0,
                "history_limit must be greater than zero"
            );
        }
        Command::Dashboard {
            limit,
            preview_chars,
        } => {
            ensure!(*limit > 0, "limit must be greater than zero");
            ensure!(
                *preview_chars > 0,
                "preview_chars must be greater than zero"
            );
        }
        Command::DashboardServer { port, .. } => {
            ensure!(*port > 0, "port must be greater than zero");
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

    if let Command::Weather {
        location,
        model,
        reasoning_effort,
    } = &command
    {
        let weather = weather::WeatherClient::from_env().context("failed to init weather")?;
        let location = agent_workflow::weather_location_query(location);
        let lookup = agent_workflow::current_weather_with_translation_fallback(
            &weather,
            &location,
            model,
            *reasoning_effort,
            &prompts.weather_location_normalizer,
        )
        .await
        .context("failed to fetch weather")?;

        if let Some(translated_location) = lookup.translated_location {
            eprintln!("translated weather location: {location} -> {translated_location}");
        }

        println!("{}", lookup.report.to_markdown());
        return Ok(());
    }

    if let Command::LongTermIndex { path, chunk_chars } = &command {
        let long_term = connect_long_term_memory(&cli.pgvector_url).await?;
        let indexed = long_term
            .index_markdown_path(path, *chunk_chars)
            .await
            .with_context(|| format!("failed to index markdown from {}", path.display()))?;

        println!("indexed {indexed} markdown chunks");
        return Ok(());
    }

    if let Command::LongTermSearch { query, limit } = &command {
        let long_term = connect_long_term_memory(&cli.pgvector_url).await?;
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
        let amplifier = llm::LlmClient::from_env(
            model.clone(),
            prompts.request_amplifier.clone(),
            *reasoning_effort,
        )
        .context("failed to init request amplifier")?;
        let amplified = amplifier
            .chat(&[], request)
            .await
            .context("failed to amplify user request")?;

        println!("{amplified}");
        return Ok(());
    }

    if let Command::DashboardServer { host, port } = &command {
        dashboard::run(dashboard::DashboardServerConfig {
            valkey_url: cli.valkey_url.clone(),
            namespace: cli.namespace.clone(),
            host: host.clone(),
            port: *port,
        })?;
        return Ok(());
    }

    let mut memory = ShortTermMemory::connect(&cli.valkey_url, &cli.namespace)
        .with_context(|| format!("failed to connect to Valkey at {}", cli.valkey_url))?;

    if let Command::Dashboard {
        limit,
        preview_chars,
    } = &command
    {
        let summaries = memory
            .chat_session_summaries(*limit)
            .context("failed to read Valkey chat dashboard")?;
        let daily_usage = memory
            .daily_token_usage()
            .context("failed to read Valkey token usage dashboard")?;
        print!(
            "{}",
            dashboard::render_cli_dashboard(&summaries, &daily_usage, *preview_chars)
        );
        return Ok(());
    }

    verify_long_term_memory(&cli.pgvector_url).await?;

    match command {
        Command::Tui {
            session,
            model,
            reasoning_effort,
            history_limit,
            ttl_seconds,
        } => {
            let session = session.unwrap_or_else(|| runtime_id("session"));
            save_system_prompt_yaml(DEFAULT_PROMPT_DIR, &prompts.agent)
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
                    preamble: prompts.agent.clone(),
                    amplifier_preamble: prompts.request_amplifier.clone(),
                    summary_preamble: prompts.summary_agent.clone(),
                    weather_translator_preamble: prompts.weather_location_normalizer.clone(),
                    pgvector_url: Some(cli.pgvector_url.clone()),
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
            save_system_prompt_yaml(DEFAULT_PROMPT_DIR, &prompts.agent)
                .context("failed to save system prompt YAML")?;
            let user_entry = ChatEntry::for_session(&user_id, &session, ChatRole::User, prompt);

            if agent_workflow::should_auto_summarize(&user_entry.content) {
                let response = if history.is_empty() {
                    "No saved chat history to summarize yet.".to_string()
                } else {
                    let response = agent_workflow::summarize_chat_history_with_usage(
                        &history,
                        model,
                        reasoning_effort,
                        &prompts.summary_agent,
                    )
                    .await?;
                    usage::record_token_usage(
                        &mut memory,
                        &user_id,
                        &session,
                        TokenUsageSource::SummaryAgent,
                        response.usage,
                        ttl_seconds,
                    )
                    .context("failed to save summary token usage")?;
                    response.text
                };

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
                return Ok(());
            }

            let prepared_prompt = agent_workflow::automatic_chat_prompt_with_usage(
                Some(cli.pgvector_url.as_str()),
                &history,
                &user_entry.content,
                &model,
                reasoning_effort,
                &prompts.request_amplifier,
                &prompts.weather_location_normalizer,
            )
            .await;
            record_automatic_token_usage(
                &mut memory,
                &user_id,
                &session,
                ttl_seconds,
                prepared_prompt.usage_events,
            )?;
            let llm = llm::LlmClient::from_env(model, prompts.agent.clone(), reasoning_effort)
                .context("failed to init LLM client")?;
            let response = llm
                .chat_with_usage(&history, &prepared_prompt.prompt)
                .await
                .context("failed to run agent chat")?;
            usage::record_token_usage(
                &mut memory,
                &user_id,
                &session,
                TokenUsageSource::FinalAnswer,
                response.usage,
                ttl_seconds,
            )
            .context("failed to save final answer token usage")?;

            memory
                .append_chat_entry(&session, user_entry, history_limit, ttl_seconds)
                .context("failed to save user chat history")?;
            memory
                .append_chat_entry(
                    &session,
                    ChatEntry::for_session(&user_id, &session, ChatRole::Assistant, &response.text),
                    history_limit,
                    ttl_seconds,
                )
                .context("failed to save assistant chat history")?;

            println!("{}", response.text);
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
        Command::Summary {
            session,
            model,
            reasoning_effort,
            history_limit,
        } => {
            let session = session.unwrap_or_else(|| runtime_id("session"));
            let history = memory
                .chat_history(&session)
                .context("failed to read chat history")?;
            let history = last_chat_entries(history, history_limit);

            if history.is_empty() {
                println!("not found");
            } else {
                let summary = agent_workflow::summarize_chat_history_with_usage(
                    &history,
                    model,
                    reasoning_effort,
                    &prompts.summary_agent,
                )
                .await?;
                usage::record_token_usage(
                    &mut memory,
                    &user_id,
                    &session,
                    TokenUsageSource::SummaryAgent,
                    summary.usage,
                    cli::DEFAULT_CHAT_TTL_SECONDS,
                )
                .context("failed to save summary token usage")?;

                println!("{}", summary.text);
            }
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
        Command::Dashboard { .. } => unreachable!("dashboard is handled before pgvector connects"),
        Command::DashboardServer { .. } => {
            unreachable!("dashboard server is handled before Valkey connects")
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

async fn connect_long_term_memory(pgvector_url: &str) -> Result<long_term_memory::LongTermMemory> {
    long_term_memory::connect(pgvector_url)
        .await
        .with_context(|| format!("failed to connect to pgvector at {pgvector_url}"))
}

async fn verify_long_term_memory(pgvector_url: &str) -> Result<()> {
    let long_term = connect_long_term_memory(pgvector_url).await?;
    long_term
        .init()
        .await
        .with_context(|| format!("failed to initialize pgvector schema at {pgvector_url}"))?;

    Ok(())
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

fn last_chat_entries(mut entries: Vec<ChatEntry>, limit: usize) -> Vec<ChatEntry> {
    let start = entries.len().saturating_sub(limit);
    entries.split_off(start)
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

fn record_automatic_token_usage(
    memory: &mut ShortTermMemory,
    user_id: &str,
    session: &str,
    ttl_seconds: u64,
    usage_events: Vec<agent_workflow::AutomaticTokenUsage>,
) -> Result<()> {
    for event in usage_events {
        usage::record_token_usage(
            memory,
            user_id,
            session,
            event.source,
            event.usage,
            ttl_seconds,
        )
        .context("failed to save automatic token usage")?;
    }

    Ok(())
}
