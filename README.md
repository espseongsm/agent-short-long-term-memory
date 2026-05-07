# Agent short and long term memory

This project is a Rust terminal agent for experimenting with short term and long term memory.

Current scope:

- Build a simple agent with Rig workflow messages and Rig's OpenAI-compatible provider for model calls.
- Use a TUI as the agent interface.
- Use Valkey as Redis-compatible short term memory.
- Save chat history in Valkey with user id, session id, timestamp, role, and content.
- Save the agent system prompt as YAML in a local `prompt/` folder.
- Control LLM reasoning effort for latency when the provider supports it.
- Automatically fetch current weather from a dedicated weather API for weather requests.
- Automatically search the web through a full Brave Search API integration for current or explicitly web-backed requests.
- Automatically amplify rough user requests when they are too vague to answer well.
- Use pgvector as long term memory over local Markdown files.
- Search saved chat history.
- Keep daily development progress reports in Markdown.
- Keep `src/main.rs` as the entry point and keep feature logic in small modules.

## What has been done

- Added a PRD in `prd.md`.
- Added Valkey-backed short term memory in `src/lib.rs`.
- Added structured chat history entries with user id, session id, timestamp, and `user` or `assistant` roles.
- Added generated runtime user ids and session ids for default runs.
- Added local YAML archiving for the system prompt in `prompt/`.
- Added conversion from saved chat entries into Rig workflow messages.
- Added case-insensitive chat history search.
- Added a Ratatui-based terminal UI in `src/tui.rs`.
- Added English/Korean Unicode input handling in the TUI prompt line.
- Added TUI conversation scrolling for session history.
- Added basic Markdown rendering for conversation messages.
- Added live TUI status and kept in-conversation LLM activity lines for model responses.
- Added an Open-Meteo-backed current weather tool with automatic chat enrichment and CLI access.
- Added a Brave Search API-backed web search tool with automatic chat enrichment plus CLI and TUI access.
- Added a request amplifier sub-agent with automatic vague-request enrichment plus CLI and TUI access.
- Added optional LLM reasoning effort control for latency-sensitive runs.
- Added mouse wheel scrolling for the TUI conversation pane.
- Added TUI clipboard capture for `Ctrl+C`, `Ctrl+V`, and bracketed paste.
- Showed submitted TUI user messages immediately while the model response is still pending.
- Added pgvector-backed long term memory indexing and search for local Markdown files.
- Added a single-file daily development progress report in `daily-progress-report.md`.
- Added GitHub Actions CI for formatting, tests, clippy, Valkey integration tests, and CLI help smoke checks.
- Kept CLI subcommands in `src/main.rs` for utility and scripting workflows.
- Moved automatic chat prompt enrichment into `src/agent_workflow.rs` so `src/main.rs` stays focused on CLI dispatch.
- Kept local sensitive variables in `.env`, and ensured `.env` is ignored by git.
- Added Rig Core, Tokio, Serde, Serde JSON, Redis, Reqwest, Clap, Anyhow, Thiserror, Ratatui, Crossterm, Arboard, Tokio Postgres, and Dotenvy dependencies.
- Started and verified a local Valkey Docker container named `valkey-memory`.

## Code layout

```text
src/main.rs              CLI entry point and command dispatch
src/tui.rs               Ratatui interface, input handling, and live status rendering
src/agent_workflow.rs    Automatic routing, prompt enrichment, and workflow helpers
src/llm.rs               Rig OpenAI-compatible model client and message shaping
src/lib.rs               Valkey-backed short term memory and prompt YAML archiving
src/long_term_memory.rs  pgvector Markdown indexing and search
src/weather.rs           Open-Meteo current weather tool
src/web_search.rs        Brave Search API web search tool
```

## Architecture

```text
TUI
 |
|-- immediate user message display
|-- prompt/system.yaml system prompt archive
|-- optional reasoning effort control
|-- kept in-conversation LLM activity lines
|-- automatic current weather router plus weather CLI
|-- automatic web search router plus /search command
|-- automatic request amplifier router plus /amplify command
|-- TUI clipboard capture for Ctrl+C, Ctrl+V, bracketed paste
 |
 v
Rig workflow messages
 |
 v
Rig OpenAI-compatible provider
 |
 v
Qwen/Qwen3.6-35B-A3B
 ^
 |
Valkey chat history with user/session/timestamp metadata
 |
 |-- CLI history/search
 |-- CLI remember/recall/forget
 |-- final answers enriched by automatic weather context
 |-- manual CLI/TUI web search results
 |-- manual CLI/TUI amplified requests
 |-- final answers enriched by automatic web/amplifier context
 |
 v
pgvector long term memory
 |
 |-- local Markdown indexing
 |-- CLI long-term-index/search
 |-- optional automatic chat context when PGVECTOR_URL is set
```

## Valkey

The default Valkey URL is:

```sh
redis://127.0.0.1:6379/
```

The current local server was started with Docker:

```sh
docker run -d --name valkey-memory -p 6379:6379 valkey/valkey:latest
```

If the container already exists, start it instead:

```sh
docker start valkey-memory
```

Check that it is running:

```sh
docker ps --filter name=valkey-memory
docker exec valkey-memory valkey-cli ping
```

Expected ping result:

```text
PONG
```

If you want `valkey-cli` directly on macOS:

```sh
brew install valkey
```

## TUI usage

Start the terminal UI:

```sh
cargo run
```

The app automatically loads `.env` when it exists. It reads model, Valkey, weather, web-search, and pgvector settings from the process environment, so exported or inline variables still work.
By default, each process gets a fresh generated user id and session id. Pass `--user-id` and `--session`, or set `AGENT_USER_ID`, when you want to continue a known history.

Start the terminal UI with explicit options:

```sh
cargo run -- --user-id soonmo tui --session work --model Qwen/Qwen3.6-35B-A3B --reasoning-effort low
```

The TUI loads recent chat history from Valkey, shapes the prompt flow as Rig workflow messages, sends the request through Rig's OpenAI-compatible provider, and stores the user and assistant messages back into Valkey.
The TUI marks the status line as `Rust | Rig OpenAI-compatible provider`.
While a response is pending, the conversation pane shows an assistant activity line between the submitted user message and the final assistant response. The activity line is kept in the conversation pane after the response arrives, while Valkey stores only user and assistant chat messages. The status line also shows the model-call flow and elapsed wait time.
Reasoning effort is optional and is not sent by default. Use `--reasoning-effort` or `LLM_REASONING_EFFORT` when the selected provider supports it. Inside the TUI, run `/reasoning <unset|none|minimal|low|medium|high|xhigh>` or `/effort <value>` to change the value for future model and amplifier calls in the current session.
The conversation pane can be scrolled with Up/Down, PageUp/PageDown, Home, End, and the mouse wheel.
Conversation messages are rendered with basic Markdown support for headings, lists, quotes, code, and emphasis.
`Ctrl+C` copies the current prompt when the prompt line has text, and exits the TUI when the prompt line is empty. `Ctrl+V` and bracketed paste insert clipboard text into the prompt, preserving pasted newlines as spaces.
Normal chat automatically fetches current weather from Open-Meteo for weather prompts such as `Seoul current weather` or `서울 현재 날씨`, then passes the weather report into the model as context.
Normal chat automatically searches the web for current information requests such as latest/current/news/price prompts, then passes the result summary into the model as context.
Normal chat automatically asks the request amplifier sub-agent to expand vague requests such as `make it better`, then asks the main model to answer the amplified request while preserving the original intent.
When `PGVECTOR_URL` is set and Markdown has been indexed, normal chat also searches long term memory and passes the most relevant local Markdown chunks into the model as optional context.
Run `/search <query>` or `/web <query>` in the TUI to force a standalone web search. Search requests and result summaries are saved as user/assistant messages; the web-search action line is display-only.
Run `/amplify <request>` in the TUI to force a standalone request amplification. Amplification requests and results are saved as user/assistant messages; the amplifier action line is display-only.

The agent system prompt is saved to `prompt/system.yaml`. Chat history is saved in Valkey, not in `prompt/`.

Example `.env` keys:

```sh
AGENT_USER_ID=soonmo
OPENAI_API_KEY=...
OPENAI_BASE_URL=...
OPENAI_MODEL=Qwen/Qwen3.6-35B-A3B
LLM_REASONING_EFFORT=low
LLM_TIMEOUT_SECONDS=30
BRAVE_SEARCH_API_KEY=...
WEB_SEARCH_URL=https://api.search.brave.com/res/v1/web/search
WEB_SEARCH_TIMEOUT_SECONDS=10
WEATHER_GEOCODING_URL=http://geocoding-api.open-meteo.com/v1/search
WEATHER_FORECAST_URL=http://api.open-meteo.com/v1/forecast
WEATHER_TIMEOUT_SECONDS=10
PGVECTOR_URL=postgres://postgres:postgres@127.0.0.1:5432/agent_memory
```

`BEARER_TOKEN` is also accepted as a fallback when `OPENAI_API_KEY` is not set.
If a local `.env` uses `OPENAI_API_KEY` for the provider URL, the app treats that URL as the API base and uses `BEARER_TOKEN` as the bearer key.

## CLI utility usage

Show help:

```sh
cargo run -- --help
```

Run one chat turn without opening the TUI:

```sh
cargo run -- chat "hello"
```

The `chat` command uses the same automatic weather, web-search, request-amplifier, and optional pgvector long term memory routing as the TUI. Automatic helper failures are reported to stderr and the main model continues with the best available prompt.

Use a named chat session:

```sh
cargo run -- --user-id soonmo chat "hello" --session work
```

Read chat history:

```sh
cargo run -- history
cargo run -- history --session work
```

Search chat history:

```sh
cargo run -- search hello
cargo run -- search hello --session work
```

Search the web:

```sh
BRAVE_SEARCH_API_KEY=... cargo run -- web-search "latest Rust release"
BRAVE_SEARCH_API_KEY=... cargo run -- web-search "Valkey Redis fork" --limit 3
```

Fetch current weather:

```sh
cargo run -- weather Seoul
```

Index local Markdown into pgvector long term memory:

```sh
PGVECTOR_URL=postgres://postgres:postgres@127.0.0.1:5432/agent_memory cargo run -- long-term-index ./notes
```

Search long term memory:

```sh
PGVECTOR_URL=postgres://postgres:postgres@127.0.0.1:5432/agent_memory cargo run -- long-term-search "Valkey setup" --limit 3
```

The pgvector schema is created automatically. The configured PostgreSQL role needs permission to run `CREATE EXTENSION IF NOT EXISTS vector`. Markdown chunks use deterministic local hashed embeddings, so indexing works without a separate embeddings API.

Amplify a request:

```sh
cargo run -- amplify "make the tui better"
```

Save, read, and delete a simple short term memory value:

```sh
cargo run -- remember smoke:value ok --ttl-seconds 60
cargo run -- recall smoke:value
cargo run -- forget smoke:value
```

Change the OpenAI model:

```sh
OPENAI_MODEL=Qwen/Qwen3.6-35B-A3B cargo run -- chat "hello"
```

Control reasoning effort for a latency-sensitive chat turn:

```sh
cargo run -- chat "hello" --reasoning-effort low
```

Control reasoning effort inside the TUI:

```text
/reasoning low
/reasoning unset
```

## Daily reports

Development progress is saved in Korean in one Markdown file: `daily-progress-report.md`. Add each working day as a dated section with at most five concise bullet points.

## CI

GitHub Actions runs `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, Valkey-backed ignored tests, and CLI/TUI help smoke checks on pushes to `main` and pull requests.

## Defaults

- Valkey URL: `redis://127.0.0.1:6379/`
- Namespace: `agent:short-term`
- User id: generated per process unless `--user-id` or `AGENT_USER_ID` is set
- Chat session: generated per command unless `--session` is set
- Chat history limit: `20`
- Chat TTL: `86400` seconds
- OpenAI model: `Qwen/Qwen3.6-35B-A3B`
- Reasoning effort: unset unless `--reasoning-effort` or `LLM_REASONING_EFFORT` is provided
- Weather API: Open-Meteo geocoding and forecast endpoints
- Web search URL: `https://api.search.brave.com/res/v1/web/search`
- Web search limit: `5`
- Web search API key: `BRAVE_SEARCH_API_KEY` or `WEB_SEARCH_API_KEY`
- pgvector URL: unset by default; set `PGVECTOR_URL` to enable long term memory

## Verification

These checks passed:

```sh
cargo fmt --check
CARGO_TARGET_DIR=target/codex-rig-search cargo test
CARGO_TARGET_DIR=target/codex-rig-search cargo clippy -- -D warnings
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- --help
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- tui --help
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- web-search --help
cargo tree | rg "async-openai|rig-core"
git diff --check
```

Live TUI, live pgvector indexing/search, Brave search requests, and `chat` model calls require local services or provider credentials and have not been verified in this session.
