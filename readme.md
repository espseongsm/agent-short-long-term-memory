# Agent short and long term memory

This project is a Rust terminal agent for experimenting with short term and long term memory.

Current scope:

- Build a simple agent with Rig workflow messages and an OpenAI-compatible SDK for model calls.
- Use a TUI as the agent interface.
- Use Valkey as Redis-compatible short term memory.
- Save chat history in Valkey with user id, session id, timestamp, role, and content.
- Save the agent system prompt as YAML in a local `prompt/` folder.
- Search saved chat history.
- Keep daily development progress reports in Markdown.
- Leave long term memory design open until storage and retrieval behavior is defined.

## What has been done

- Added a PRD in `prd.md`.
- Added Valkey-backed short term memory in `src/lib.rs`.
- Added structured chat history entries with user id, session id, timestamp, and `user` or `assistant` roles.
- Added generated runtime user ids and session ids for default runs.
- Added local YAML archiving for the system prompt in `prompt/`.
- Added conversion from saved chat entries into Rig workflow messages and OpenAI chat completion messages.
- Added case-insensitive chat history search.
- Added a Ratatui-based terminal UI in `src/tui.rs`.
- Added English/Korean Unicode input handling in the TUI prompt line.
- Added TUI conversation scrolling for session history.
- Added basic Markdown rendering for conversation messages.
- Added live TUI status while model responses are pending.
- Added mouse wheel scrolling for the TUI conversation pane.
- Showed submitted TUI user messages immediately while the model response is still pending.
- Added a single-file daily development progress report in `daily-progress-report.md`.
- Kept CLI subcommands in `src/main.rs` for utility and scripting workflows.
- Kept local sensitive variables in `.env`, and ensured `.env` is ignored by git.
- Added Rig Core, Async OpenAI, Tokio, Serde, Serde JSON, Redis, Clap, Anyhow, Thiserror, Ratatui, Crossterm, and Dotenvy dependencies.
- Started and verified a local Valkey Docker container named `valkey-memory`.

## Architecture

```text
TUI
 |
|-- immediate user message display
|-- prompt/system.yaml system prompt archive
 |
 v
Rig workflow messages
 |
 v
OpenAI-compatible SDK
 |
 v
Qwen/Qwen3.6-35B-A3B
 ^
 |
Valkey chat history with user/session/timestamp metadata
 |
 |-- CLI history/search
 |-- CLI remember/recall/forget
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

The app automatically loads `.env` when it exists. It reads `OPENAI_API_KEY`, `BEARER_TOKEN`, `OPENAI_BASE_URL`, `OPENAI_API_BASE`, `OPENAI_MODEL`, and `LLM_TIMEOUT_SECONDS` from the process environment, so exported or inline variables still work.
By default, each process gets a fresh generated user id and session id. Pass `--user-id` and `--session`, or set `AGENT_USER_ID`, when you want to continue a known history.

Start the terminal UI with explicit options:

```sh
cargo run -- --user-id soonmo tui --session work --model Qwen/Qwen3.6-35B-A3B
```

The TUI loads recent chat history from Valkey, shapes the prompt flow as Rig workflow messages, sends the request through the OpenAI-compatible SDK, and stores the user and assistant messages back into Valkey.
The TUI marks the status line as `Rust | Rig workflow + OpenAI SDK`.
While a response is pending, the status line shows the model-call flow and elapsed wait time.
The conversation pane can be scrolled with Up/Down, PageUp/PageDown, Home, End, and the mouse wheel.
Conversation messages are rendered with basic Markdown support for headings, lists, quotes, code, and emphasis.

The agent system prompt is saved to `prompt/system.yaml`. Chat history is saved in Valkey, not in `prompt/`.

Example `.env` keys:

```sh
AGENT_USER_ID=soonmo
OPENAI_API_KEY=...
OPENAI_BASE_URL=...
OPENAI_MODEL=Qwen/Qwen3.6-35B-A3B
LLM_TIMEOUT_SECONDS=30
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

## Daily reports

Development progress is saved in Korean in one Markdown file: `daily-progress-report.md`. Add each working day as a dated section with at most five concise bullet points.

## Defaults

- Valkey URL: `redis://127.0.0.1:6379/`
- Namespace: `agent:short-term`
- User id: generated per process unless `--user-id` or `AGENT_USER_ID` is set
- Chat session: generated per command unless `--session` is set
- Chat history limit: `20`
- Chat TTL: `86400` seconds
- OpenAI model: `Qwen/Qwen3.6-35B-A3B`

## Verification

These checks passed:

```sh
cargo fmt
cargo test
VALKEY_URL=redis://127.0.0.1:6379/ cargo test -- --ignored
cargo clippy -- -D warnings
cargo run -- --help
cargo run -- tui --help
```

Live TUI and `chat` model calls have not been verified in this session.

## Current limitation

Long term memory is only documented as a future requirement. The storage backend, retrieval strategy, and promotion rules from short term memory to long term memory still need to be defined.
