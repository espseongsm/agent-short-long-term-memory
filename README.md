# Agent short and long term memory

This project is a Rust terminal agent for experimenting with short term and long term memory.

Current scope:

- Build a simple agent with Rig workflow messages and Rig's OpenAI-compatible provider for model calls.
- Use a TUI as the agent interface.
- Use Valkey as Redis-compatible short term memory.
- Save chat history in Valkey with user id, session id, timestamp, role, and content.
- Keep agent and sub-agent prompts as files in the local `prompt/` folder.
- Control LLM reasoning effort for latency when the provider supports it.
- Automatically fetch current weather from a dedicated weather API for weather requests.
- Automatically search the web through a full Brave Search API integration for current or explicitly web-backed requests.
- Automatically amplify rough user requests when they are too vague to answer well.
- Automatically summarize saved chat sessions through a dedicated summary sub-agent when the user asks for a recap.
- Use pgvector as long term memory over local Markdown files.
- Search saved chat history.
- Keep daily development progress reports in Markdown.
- Keep `src/main.rs` as the entry point and keep feature logic in small modules.

## What has been done

- Added a PRD in `prd.md`.
- Added Valkey-backed short term memory in `src/lib.rs`.
- Added structured chat history entries with user id, session id, timestamp, and `user` or `assistant` roles.
- Added generated runtime user ids and session ids for default runs.
- Added file-backed prompts in `prompt/` and local YAML archiving for the main system prompt.
- Added conversion from saved chat entries into Rig workflow messages.
- Added case-insensitive chat history search.
- Added a Ratatui-based terminal UI in `src/tui.rs`.
- Added English/Korean Unicode input handling in the TUI prompt line.
- Added TUI conversation scrolling for session history.
- Added basic Markdown rendering for conversation messages.
- Added live TUI status and kept in-conversation LLM activity lines with two-decimal elapsed time for model responses.
- Added TUI session token usage display from Rig model responses.
- Added an Open-Meteo-backed current weather tool with automatic chat enrichment, CLI access, Korean aliases, and an LLM location-name fallback for non-English places.
- Added a Brave Search API-backed web search tool with automatic chat enrichment plus CLI and TUI access.
- Added a request amplifier sub-agent with automatic vague-request enrichment plus CLI and TUI access.
- Added a conversation summary sub-agent with automatic routing plus CLI and TUI access.
- Added optional LLM reasoning effort control for latency-sensitive runs.
- Added mouse wheel scrolling for the TUI conversation pane.
- Added TUI clipboard capture for `Ctrl+C`, `Ctrl+V`, bracketed paste, and mouse-drag conversation selection.
- Showed submitted TUI user messages immediately while the model response is still pending.
- Added pgvector-backed long term memory indexing and search for local Markdown files.
- Added a single-file daily development progress report in `daily-progress-report.md`.
- Added GitHub Actions CI for formatting, tests, clippy, Valkey integration tests, and CLI help smoke checks.
- Kept CLI subcommands in `src/main.rs` for utility and scripting workflows.
- Moved automatic chat prompt enrichment into `src/agent_workflow.rs` so `src/main.rs` stays focused on CLI dispatch.
- Split long source files into focused `cli`, `agent_workflow`, and `tui` modules while preserving the public CLI/TUI behavior.
- Kept local sensitive variables in `.env`, and ensured `.env` is ignored by git.
- Added Rig Core, Tokio, Serde, Serde JSON, Redis, Reqwest, Clap, Anyhow, Thiserror, Ratatui, Crossterm, Arboard, Tokio Postgres, and Dotenvy dependencies.
- Started and verified a local Valkey Docker container named `valkey-memory`.

## Code layout

```text
src/main.rs              CLI command dispatch and service wiring
src/cli.rs               CLI definitions, defaults, and runtime ids
src/tui.rs               TUI facade exporting run/config
src/tui/                 TUI runtime, context, rendering, Markdown, input, commands, state
src/agent_workflow.rs    Agent workflow facade
src/agent_workflow/      Automatic routing, prompt enrichment, summary, weather fallback, locations
src/llm.rs               Rig OpenAI-compatible model client and message shaping
src/lib.rs               Valkey-backed short term memory and prompt YAML archiving
src/long_term_memory.rs  pgvector Markdown indexing and search
src/weather.rs           Open-Meteo current weather tool
src/web_search.rs        Brave Search API web search tool
prompt/*.yaml           Main agent and sub-agent prompts
```

## Architecture

```text
TUI
 |
|-- immediate user message display
|-- prompt/*.yaml agent and sub-agent prompts
|-- prompt/system.yaml generated system prompt archive
|-- optional reasoning effort control
|-- session token usage from Rig model responses
|-- kept in-conversation LLM activity lines
|-- automatic current weather router plus weather CLI
|   |-- cleanup/alias -> Open-Meteo -> LLM location normalizer -> Open-Meteo retry
|-- automatic web search router plus /search command
|-- automatic request amplifier router plus /amplify command
|-- automatic conversation summary router plus /summary command
|-- TUI clipboard capture for Ctrl+C, Ctrl+V, bracketed paste, mouse selection
 |
 v
Rig workflow messages
 |
 v
Rig OpenAI-compatible provider
 |
 v
gpt-5.5
 ^
 |
Valkey chat history with user/session/timestamp metadata
 |
 |-- CLI history/search
 |-- CLI remember/recall/forget
 |-- final answers enriched by automatic weather context
 |-- manual CLI/TUI web search results
 |-- manual CLI/TUI amplified requests
 |-- automatic and manual CLI/TUI conversation summaries
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

### Before you start

The TUI needs Valkey for chat history and an OpenAI-compatible model endpoint for assistant replies.
The app automatically loads `.env` when it exists, so you can keep local URLs and keys there instead of exporting them each time.

Minimum useful `.env` values:

```sh
OPENAI_API_KEY=...
OPENAI_BASE_URL=...
OPENAI_MODEL=gpt-5.5
```

Optional helpers:

```sh
BRAVE_SEARCH_API_KEY=...
PGVECTOR_URL=postgres://postgres:postgres@127.0.0.1:5432/agent_memory
```

By default, each `cargo run` creates a fresh user id and session id. Use `--user-id` and `--session` when you want to continue a known history.

### Start the TUI

For a fresh session:

```sh
cargo run
```

For a named session:

```sh
cargo run -- --user-id soonmo tui --session work --model gpt-5.5 --reasoning-effort low
```

### Inside the TUI

Type a message and press Enter. Your message appears immediately, then the conversation pane keeps a live assistant activity line with elapsed time, such as `2.12s`, while the model is working. The status line shows cumulative session token usage when the provider returns usage metrics.

Common controls:

```text
Up / Down            scroll conversation one row
PageUp / PageDown    scroll conversation faster
Home / End           jump to top or bottom
Mouse wheel          scroll conversation
Mouse drag           copy selected conversation text
Ctrl+V               paste clipboard text
Ctrl+C               copy the prompt when input has text
Ctrl+C               exit when input is empty
Esc                  exit
```

Useful TUI commands:

```text
/reasoning low       set reasoning effort for future model calls
/reasoning unset     stop sending reasoning effort
/copy                copy conversation, status, token usage, model, and session
/search <query>      force a standalone Brave web search
/web <query>         same as /search
/amplify <request>   force request amplification
/summary             summarize the current saved conversation
```

Normal chat automatically adds extra context when it helps:

- Weather questions call Open-Meteo first, for example `what's the weather today in Seoul?`, `Seoul current weather`, or `서울 현재 날씨`.
- After a weather question, short follow-ups such as `부산은?` or `what about Busan?` are treated as weather questions for the new location.
- Pronoun weather follow-ups such as `how's the weather of it?` and `weather there?` reuse the most recent place discussed in the chat.
- Common Korean weather locations are normalized before geocoding, for example `오늘 서울날씨는?` uses `Seoul` and `부에노스아이레스 날씨는?` uses `Buenos Aires`.
- If direct geocoding fails for a Korean or other non-English location, only the extracted location name is sent to a small weather location normalizer. For example `치앙마이 날씨는?` can retry Open-Meteo as `Chiang Mai` when model credentials are configured.
- Current-information prompts call Brave Search first, for example `latest Rust release`.
- Vague requests call the request amplifier first, for example `make it better`.
- Conversation summary requests call the summary sub-agent first, for example `summarize our conversation`, `sum up what we talked about`, or `이 대화 요약해줘`.
- If `PGVECTOR_URL` is set and Markdown has been indexed, local Markdown memory is searched first.

### Request flow

```text
User input
 |
 |-- optional summary / weather / web search / amplifier / long-term memory routing
 |
 v
Rig workflow messages
 |
 v
Rig OpenAI-compatible provider
 |
 v
Assistant response
 |
 v
Valkey chat history
```

Agent and sub-agent prompts live in `prompt/*.yaml` using the same `role` and `prompt` format as `prompt/system.yaml`. The generated main system prompt archive is saved to `prompt/system.yaml`. Chat history is saved in Valkey, not in `prompt/`.

Example `.env` keys:

```sh
AGENT_USER_ID=soonmo
OPENAI_API_KEY=...
OPENAI_BASE_URL=...
OPENAI_MODEL=gpt-5.5
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
cargo run -- weather 부산은
cargo run -- weather "오늘 서울날씨는?"
cargo run -- weather "부에노스아이레스 날씨는?"
cargo run -- weather "아르헨티나 날씨는?"
cargo run -- weather "치앙마이 날씨는?" --model gpt-5.5 --reasoning-effort low
```

The `weather` command uses deterministic cleanup and aliases first. If Open-Meteo cannot geocode the location directly, it can use the same model credentials as chat to translate only the extracted location name, then retries Open-Meteo once.

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

Summarize a saved chat session:

```sh
cargo run -- summary --session work --reasoning-effort low
```

Save, read, and delete a simple short term memory value:

```sh
cargo run -- remember smoke:value ok --ttl-seconds 60
cargo run -- recall smoke:value
cargo run -- forget smoke:value
```

Change the OpenAI model:

```sh
OPENAI_MODEL=gpt-5.5 cargo run -- chat "hello"
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
- OpenAI model: `gpt-5.5`
- Reasoning effort: unset unless `--reasoning-effort` or `LLM_REASONING_EFFORT` is provided
- Weather API: Open-Meteo geocoding and forecast endpoints
- Weather location fallback: uses `OPENAI_MODEL` or `--model` and optional `LLM_REASONING_EFFORT` or `--reasoning-effort`
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

Interactive TUI, live pgvector indexing/search, and Brave search requests require local services or provider credentials and have not been verified in this session. Weather CLI and `chat` weather fallback were verified with model credentials.
