# Agent Short And Long Term Memory

Rust terminal agent for experimenting with short term chat memory, long term local
Markdown memory, and tool-assisted agent workflows.

The app is built around a Ratatui TUI, Valkey-backed chat history, pgvector-backed
long term memory, and Rig's OpenAI-compatible provider path.

## What It Does

- Runs a TUI-first chat agent with English and Korean input.
- Stores short term chat history in Valkey with user id, session id, timestamp,
  role, and content.
- Provides Valkey chat dashboards for scanning sessions in the CLI and reading
  full conversations in a local browser.
- Shows UTC daily token usage grouped by date and session in both Valkey
  dashboards for session-backed model calls.
- Lets the web dashboard reorder conversation headers by date, session, and user.
- Indexes local Markdown files into pgvector for long term memory.
- Calls Rig workflow messages through an OpenAI-compatible model provider.
- Routes useful context automatically before the final answer:
  - current weather through Open-Meteo
  - web search through Brave Search API
  - vague request amplification through a sub-agent
  - saved conversation summaries through a summary sub-agent
  - local Markdown long term memory through pgvector for memory/notes requests
- Defaults location-free weather prompts, including greeting-prefixed prompts
  like `hello what's the weather today?`, to Seoul instead of treating greeting
  words as place names.
- Keeps agent and sub-agent prompts in `prompt/*.yaml`.
- Shows model activity, elapsed time, session token usage, and a Rust + Rig
  runtime banner in the TUI.

## Quick Start

Install Rust, clone the repo, start Valkey, configure a model endpoint, then run
the TUI.

```sh
git clone https://github.com/espseongsm/agent-short-long-term-memory.git
cd agent-short-long-term-memory
cargo build
docker run -d --name valkey-memory -p 6379:6379 valkey/valkey:latest
```

Create `.env`:

```sh
OPENAI_API_KEY=...
OPENAI_BASE_URL=...
OPENAI_MODEL=gpt-5.5
```

Start the app:

```sh
cargo run
```

Use a stable user and session when you want resumable history:

```sh
cargo run -- --user-id soonmo tui --session work --model gpt-5.5 --reasoning-effort low
```

## Architecture

### Runtime Flow

```mermaid
flowchart TD
    User[User] --> Interface{TUI or chat CLI}
    User --> DashboardCli[dashboard CLI]
    User --> DashboardWeb[dashboard-server browser UI]
    Interface --> LocalEcho[Show user message immediately]
    LocalEcho --> Router[agent_workflow automatic router]

    Router --> Summary{Summary request?}
    Router --> Weather{Weather request?}
    Router --> Web{Current web context?}
    Router --> Amplifier{Vague request?}
    Router --> Ltm{Memory or notes request?}

    Summary -->|yes| SummaryAgent[summary agent prompt]
    Weather -->|yes| WeatherLookup[Open-Meteo weather lookup]
    Web -->|yes| Brave[Brave Search API]
    Amplifier -->|yes| AmplifierAgent[request amplifier prompt]
    Ltm -->|yes and pgvector available| PgSearch[pgvector Markdown search]

    SummaryAgent --> Prompt[Final enriched prompt]
    WeatherLookup --> Prompt
    Brave --> Prompt
    AmplifierAgent --> Prompt
    PgSearch --> Prompt
    Router --> Prompt

    Prompt --> RigMessages[Rig workflow messages]
    RigMessages --> Provider[Rig OpenAI-compatible provider]
    Provider --> Model[Configured model]
    Model --> Assistant[Assistant response plus token usage]
    Assistant --> Valkey[(Valkey chat history and token usage)]
    Assistant --> Interface
    DashboardCli --> Valkey
    Valkey --> DashboardCli
    DashboardWeb --> Valkey
    Valkey --> DashboardWeb
```

### Module Map

```mermaid
flowchart LR
    main[src/main.rs] --> cli[src/cli.rs]
    main --> dashboard[src/dashboard.rs]
    main --> tuiFacade[src/tui.rs]
    main --> workflowFacade[src/agent_workflow.rs]
    main --> memory[src/lib.rs]
    main --> llm[src/llm.rs]
    main --> ltm[src/long_term_memory.rs]
    main --> weather[src/weather.rs]
    main --> web[src/web_search.rs]

    tuiFacade --> tuiRuntime[src/tui/runtime.rs]
    tuiFacade --> tuiContext[src/tui/context.rs]
    tuiFacade --> tuiRender[src/tui/render.rs]
    tuiRender --> tuiRenderModules[src/tui/render/*]
    tuiFacade --> tuiCommands[src/tui/commands.rs]
    tuiFacade --> tuiInteraction[src/tui/interaction.rs]
    tuiFacade --> tuiMarkdown[src/tui/markdown.rs]
    tuiFacade --> tuiState[src/tui/state.rs]

    workflowFacade --> actions[src/agent_workflow/actions.rs]
    actions --> actionModules[src/agent_workflow/actions/*]
    workflowFacade --> prompts[src/agent_workflow/prompts.rs]
    workflowFacade --> summary[src/agent_workflow/summary.rs]
    workflowFacade --> weatherLookup[src/agent_workflow/weather_lookup.rs]
    workflowFacade --> weatherLocations[src/agent_workflow/weather_locations.rs]
    weatherLocations --> weatherLocationModules[src/agent_workflow/weather_locations/*]

    promptsDir[prompt/*.yaml] --> llm
    dashboard --> memory
```

### Weather Lookup Fallback

```mermaid
sequenceDiagram
    participant User
    participant Router as agent_workflow
    participant Locations as weather_locations
    participant OpenMeteo as Open-Meteo
    participant Normalizer as weather location normalizer
    participant Model as Rig model provider

    User->>Router: "치앙마이 날씨는?"
    Router->>Locations: extract location
    Locations-->>Router: "치앙마이"
    Router->>OpenMeteo: direct geocoding lookup
    alt direct lookup succeeds
        OpenMeteo-->>Router: current weather
    else direct lookup fails
        Router->>Normalizer: normalize only the location name
        Normalizer->>Model: one English geocoding name
        Model-->>Normalizer: "Chiang Mai"
        Normalizer-->>Router: translated location
        Router->>OpenMeteo: retry once
        OpenMeteo-->>Router: current weather
    end
```

### Location-Free Weather Guard

```mermaid
flowchart LR
    Prompt["hello what's the weather today?"] --> Cleanup[Remove weather words]
    Cleanup --> Residue["hello"]
    Residue --> Guard[Greeting residue is not a location]
    Guard --> Default["Default location: Seoul"]
    Default --> Weather[Open-Meteo current weather]
```

## Project Layout

```text
src/main.rs              command dispatch and service wiring
src/cli.rs               CLI definitions, defaults, runtime ids, prompt loading
src/dashboard.rs         Valkey CLI/browser dashboard rendering
src/usage.rs             token usage recording helpers for model calls
src/tui.rs               TUI facade exporting run/config
src/tui/                 TUI runtime, context, render, commands, input, state
src/tui/render/          render tests/support modules
src/agent_workflow.rs    agent workflow facade
src/agent_workflow/      routing, prompt enrichment, summaries, weather fallback
src/agent_workflow/actions/ weather/web/amplify/summary/memory routing modules
src/agent_workflow/weather_locations/ weather aliases and location tests
src/llm.rs               Rig model client, message shaping, token usage
src/lib.rs               Valkey short term memory, usage events, prompt YAML archiving
src/token_usage.rs       UTC daily token usage aggregation
src/long_term_memory.rs  pgvector Markdown indexing and search
src/weather.rs           Open-Meteo current weather client
src/web_search.rs        Brave Search API client
prompt/*.yaml           agent and sub-agent prompts
prd.md                  product requirements and current status
daily-progress-report.md Korean daily development notes
```

## Configuration

The app loads `.env` automatically when present.

Minimum model configuration:

```sh
OPENAI_API_KEY=...
OPENAI_BASE_URL=...
OPENAI_MODEL=gpt-5.5
```

Optional configuration:

```sh
AGENT_USER_ID=soonmo
LLM_REASONING_EFFORT=low
LLM_TIMEOUT_SECONDS=30
BRAVE_SEARCH_API_KEY=...
WEB_SEARCH_URL=https://api.search.brave.com/res/v1/web/search
WEB_SEARCH_TIMEOUT_SECONDS=10
WEATHER_GEOCODING_URL=http://geocoding-api.open-meteo.com/v1/search
WEATHER_FORECAST_URL=http://api.open-meteo.com/v1/forecast
PGVECTOR_URL=postgres://postgres:postgres@127.0.0.1:5432/agent_memory
LONG_TERM_MEMORY_PATH="/Users/soonmoseong/Library/Mobile Documents/iCloud~md~obsidian/"
WEATHER_TIMEOUT_SECONDS=10
```

`BEARER_TOKEN` is accepted as a fallback when `OPENAI_API_KEY` is not set. If a
local `.env` uses `OPENAI_API_KEY` for the provider URL, the app treats that
value as the API base and uses `BEARER_TOKEN` as the bearer key.

## Install Local Services

### Valkey

The default Valkey URL is:

```sh
redis://127.0.0.1:6379/
```

Start Valkey with Docker:

```sh
docker run -d --name valkey-memory -p 6379:6379 valkey/valkey:latest
```

If the container already exists:

```sh
docker start valkey-memory
```

Check the server:

```sh
docker ps --filter name=valkey-memory
docker exec valkey-memory valkey-cli ping
```

Expected result:

```text
PONG
```

Install `valkey-cli` directly on macOS when useful:

```sh
brew install valkey
```

### pgvector

Long term memory is required for normal TUI/chat startup. The default pgvector
URL is `postgres://postgres:postgres@127.0.0.1:5432/agent_memory`, and `cargo run`
fails early if pgvector is unavailable.

Start pgvector with Docker:

```sh
docker run -d --name pgvector-memory \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=agent_memory \
  -p 5432:5432 \
  pgvector/pgvector:pg16
```

If the container already exists:

```sh
docker start pgvector-memory
```

Check the server:

```sh
docker exec pgvector-memory pg_isready -U postgres -d agent_memory
```

`long-term-index` defaults to the local Markdown source at
`/Users/soonmoseong/Library/Mobile Documents/iCloud~md~obsidian/`; set
`LONG_TERM_MEMORY_PATH` or pass a path argument to override it.

The configured PostgreSQL role must be able to run:

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Markdown chunks use deterministic local hashed embeddings, so indexing does not
need a separate embeddings API.

## TUI Usage

Start a fresh session:

```sh
cargo run
```

Start a named session:

```sh
cargo run -- --user-id soonmo tui --session work --model gpt-5.5 --reasoning-effort low
```

The TUI shows the user message immediately, keeps assistant activity lines in
the conversation while model calls are pending, and shows cumulative session
token usage when the provider returns usage metrics.

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

TUI commands:

```text
/reasoning low       set reasoning effort for future model calls
/reasoning unset     stop sending reasoning effort
/copy                copy conversation, status, token usage, model, and session
/search <query>      force a standalone Brave web search
/web <query>         same as /search
/amplify <request>   force request amplification
/summary             summarize the current saved conversation
```

Automatic context routing:

| User intent | Automatic behavior |
| --- | --- |
| Weather question | Fetches Open-Meteo current weather. |
| Location-free weather question like `hello what's the weather today?` | Strips greeting residue and defaults to Seoul. |
| Weather follow-up like `부산은?` | Reuses weather context and treats the text as a new location. |
| Pronoun weather follow-up like `weather there?` | Resolves the latest discussed place from chat history. |
| Korean or non-English place names | Tries deterministic aliases, then LLM location normalization if needed. |
| Current-information prompt | Searches Brave before the final model answer. |
| Vague request | Calls the request amplifier sub-agent first. |
| Conversation recap request | Calls the summary sub-agent first. |
| Memory or notes request | Searches indexed Markdown memory when pgvector is available. |

## CLI Usage

Show help:

```sh
cargo run -- --help
```

Run one chat turn:

```sh
cargo run -- chat "hello"
cargo run -- --user-id soonmo chat "hello" --session work
```

Read and search chat history:

```sh
cargo run -- history
cargo run -- history --session work
cargo run -- search hello
cargo run -- search hello --session work
cargo run -- dashboard
cargo run -- dashboard --limit 20 --preview-chars 160
cargo run -- dashboard-server
```

Open the web dashboard at `http://127.0.0.1:7878`. It reads Valkey chat
sessions directly, shows UTC daily token usage with expandable date groups and
clickable session links, and lets you choose whether conversation headers group
by date, session, or user first.

Search the web:

```sh
BRAVE_SEARCH_API_KEY=... cargo run -- web-search "latest Rust release"
BRAVE_SEARCH_API_KEY=... cargo run -- web-search "Valkey Redis fork" --limit 3
```

Fetch current weather:

```sh
cargo run -- weather Seoul
cargo run -- weather 부산은
cargo run -- weather "hello what's the weather today?"
cargo run -- weather "오늘 서울날씨는?"
cargo run -- weather "부에노스아이레스 날씨는?"
cargo run -- weather "아르헨티나 날씨는?"
cargo run -- weather "치앙마이 날씨는?" --model gpt-5.5 --reasoning-effort low
```

Index and search local Markdown long term memory:

```sh
cargo run -- long-term-index
cargo run -- long-term-index ./notes
cargo run -- long-term-search "Valkey setup" --limit 3
```

Run sub-agents directly:

```sh
cargo run -- amplify "make the tui better"
cargo run -- summary --session work --reasoning-effort low
```

Use simple short term memory values:

```sh
cargo run -- remember smoke:value ok --ttl-seconds 60
cargo run -- recall smoke:value
cargo run -- forget smoke:value
```

Control model and reasoning effort:

```sh
OPENAI_MODEL=gpt-5.5 cargo run -- chat "hello"
cargo run -- chat "hello" --reasoning-effort low
```

Inside the TUI:

```text
/reasoning low
/reasoning unset
```

## Prompts

Agent prompts live in `prompt/*.yaml` with this shape:

```yaml
role: system
prompt: You are a concise helpful assistant.
```

Tracked prompt files:

- `prompt/agent.yaml`
- `prompt/request_amplifier.yaml`
- `prompt/summary_agent.yaml`
- `prompt/weather_location_normalizer.yaml`

`prompt/system.yaml` is a generated local archive of the main system prompt and
is intentionally ignored by git.

## Defaults

| Setting | Default |
| --- | --- |
| Valkey URL | `redis://127.0.0.1:6379/` |
| Namespace | `agent:short-term` |
| User id | generated per process unless `--user-id` or `AGENT_USER_ID` is set |
| Chat session | generated per command unless `--session` is set |
| Chat history limit | `20` |
| Chat TTL | `86400` seconds |
| OpenAI model | `gpt-5.5` |
| Reasoning effort | unset unless `--reasoning-effort` or `LLM_REASONING_EFFORT` is provided |
| Weather API | Open-Meteo geocoding and forecast endpoints |
| Weather fallback | deterministic cleanup and aliases, then one model-normalized retry |
| Web search URL | `https://api.search.brave.com/res/v1/web/search` |
| Web search limit | `5` |
| Web search API key | `BRAVE_SEARCH_API_KEY` or `WEB_SEARCH_API_KEY` |
| pgvector URL | `postgres://postgres:postgres@127.0.0.1:5432/agent_memory` |
| Long term Markdown path | `/Users/soonmoseong/Library/Mobile Documents/iCloud~md~obsidian/` |

## Development

Daily development notes are kept in Korean in `daily-progress-report.md`.
Product requirements and current status live in `prd.md`.

Run checks before pushing:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo run --quiet -- dashboard --limit 5 --preview-chars 80
cargo run --quiet -- weather "hello what's the weather today?" --reasoning-effort low
git diff --check
```

GitHub Actions runs formatting, tests, clippy, Valkey-backed ignored tests, and
CLI/TUI help smoke checks on pushes to `main` and pull requests.

## Verification Snapshot

The most recent local checks for this README reorganization:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
git diff --check
```
