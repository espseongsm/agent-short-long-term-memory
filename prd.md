# Agent Short and Long Term Memory PRD

## 1. Purpose

Build a Rust terminal agent that combines:

- A TUI-first chat interface.
- Short term chat memory in Valkey.
- Long term memory over local Markdown files with pgvector.
- Rig-based OpenAI-compatible model calls.
- Automatic context tools for weather, web search, vague request amplification, and local memory.
- Multi-agent-style orchestration such as automatic routing, request amplification, conversation summarization, weather location normalization, and final answer generation.

The project should stay understandable as a small Rust codebase: `src/main.rs` remains the entry point, and feature logic lives in focused modules.

## 2. Success Criteria

- A user can run the app, chat in the TUI, and see the submitted message immediately.
- The TUI clearly shows what the agent is doing while model calls are pending such as workflow, active agents, and tool calling.
- Chat history is stored with user id, session id, timestamp, role, and content.
- Sessions can be searched and resumed when the user supplies stable ids.
- The model call path uses Rig's OpenAI-compatible provider.
- Weather, web search, request amplification, conversation summary, and long term memory are added automatically when useful.
- README and the daily progress report stay current enough for future follow-up work.

## 3. Current Architecture

```text
TUI / chat CLI
 |
 |-- user input
 |-- immediate local user message
 |-- prompt/*.yaml agent and sub-agent prompts
 |-- optional automatic context routing
 |     |-- summary agent when the user asks to sum up the saved conversation
 |     |-- current weather
 |     |   |-- cleanup/alias -> Open-Meteo -> weather location normalizer -> Open-Meteo retry
 |     |   |-- missing location -> Seoul default
 |     |   |-- weather-context correction -> new location weather
 |     |-- Brave web search
 |     |-- request amplifier sub-agent
 |     |-- pgvector Markdown memory
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

## 4. Current Repo Snapshot

Status is based on the repo state on 2026-05-07.

| Area | Status | Notes |
| --- | --- | --- |
| TUI chat interface | Done | Ratatui interface with English/Korean input support. |
| Model provider | Done | Rig OpenAI-compatible provider with HTTPS/TLS support. |
| Default model | Done | `gpt-5.5`. |
| Short term memory | Done | Valkey stores chat history and simple key/value memory. |
| Long term memory | Done | pgvector indexes and searches local Markdown files. |
| Weather context | Done | Open-Meteo current weather routing, CLI access, Korean aliases, and LLM location fallback. |
| Web search context | Done | Brave Search API routing, CLI access, and TUI commands. |
| Request amplifier | Done | Sub-agent expands vague requests automatically and by command. |
| Summary agent | Done | Sub-agent summarizes saved chat sessions automatically and from CLI/TUI commands. |
| Reasoning effort | Done | CLI/env/TUI control when the provider supports it. |
| Docs | Done | README, PRD, and daily progress report describe the current module layout and behavior. |

## 5. Functional Requirements

### 5.1 Agent Workflow

| ID | Requirement | Status |
| --- | --- | --- |
| AW-1 | Use Rig as the main framework for model workflow messages and provider calls. | Done |
| AW-2 | Use Rig's OpenAI-compatible provider for model calls. | Done |
| AW-3 | Load model URL, API key, model name, and optional settings from `.env` or environment variables. | Done |
| AW-4 | Keep each agent and sub-agent prompt as a YAML file in the `prompt/` folder using `role` and `prompt` fields. | Done |
| AW-5 | Show agent activity inside the TUI conversation while long model calls are pending. | Done |
| AW-6 | Keep activity lines visible in the conversation pane after the response arrives. | Done |
| AW-7 | Support optional reasoning effort control for latency-sensitive runs. | Done |

- shows elapsed time like 2.12s
- each agent has its own prompts whether or not it's sub.
- multi-turn conversation so agent can recognize chat history and answer about previous chat turns in the same user id and session id.
- sub-agent-style helpers: request amplifier, summary agent, weather location normalizer.
- orchestrator behavior: route summary requests, weather, web search, request amplification, and long-term context before the final model answer.
- all sub-agents can be executed automatically.

### 5.2 TUI

| ID | Requirement | Status |
| --- | --- | --- |
| TUI-1 | Make the TUI the primary interface for the agent. | Done |
| TUI-2 | Support English and Korean text input. | Done |
| TUI-3 | Show the user message immediately after submission. | Done |
| TUI-4 | Support keyboard and mouse scrolling in the conversation pane. | Done |
| TUI-5 | Render basic Markdown in chat messages. | Done |
| TUI-6 | Support `Ctrl+C`, `Ctrl+V`, and bracketed paste behavior. | Done |
| TUI-7 | Provide TUI commands for reasoning effort, web search, request amplification, and summaries. | Done |
| TUI-8 | Support copy by mouse selection automatically. Show copied | Done |
| TUI-9 | Show the token usage of the session. | Done |

### 5.3 Short Term Memory

| ID | Requirement | Status |
| --- | --- | --- |
| STM-1 | Use Valkey as the short term memory database. | Done |
| STM-2 | Store chat history with user id, session id, timestamp, role, and content. | Done |
| STM-3 | Support chat history search. | Done |
| STM-4 | Generate fresh runtime user and session ids by default on each run. | Done |
| STM-5 | Allow stable user and session ids for continuing known histories. | Done |

Valkey reference: <https://valkey.io/topics/>

### 5.4 Long Term Memory

| ID | Requirement | Status |
| --- | --- | --- |
| LTM-1 | Use pgvector for long term memory. | Done |
| LTM-2 | Use local Markdown files(/Users/soonmoseong/Library/Mobile Documents/iCloud~md~obsidian/) as the data source. | Done |
| LTM-3 | Provide CLI indexing and search commands. | Done |
| LTM-4 | Add relevant long term memory context automatically when `PGVECTOR_URL` is configured. | Done |

### 5.5 Tools and Context

| ID | Requirement | Status |
| --- | --- | --- |
| TOOL-1 | Automatically call a current weather tool for weather questions. | Done |
| TOOL-2 | Automatically call a full web search API for current-information questions. | Done |
| TOOL-3 | Use Brave Search API for web search. | Done |
| TOOL-4 | Automatically call a request amplifier sub-agent for requests that are too vague. | Done |
| TOOL-5 | Preserve the user's original intent when amplifying a request. | Done |
| TOOL-6 | Automatically call a summary sub-agent when the user asks the agent to sum up the saved conversation. | Done |

- Korean weather prompts and follow-ups such as `오늘 서울날씨는?`, `부산은?`, `뉴욕은?`, and `부에노스아이레스 날씨는?` normalize Korean domestic and foreign location names before calling the weather tool.
- Location-free weather prompts such as `오늘 날씨는?` and `how's the weather today?` default to `Seoul` instead of sending question residue to the weather location normalizer.
- Pronoun weather follow-ups such as `how's the weather of it?` and `weather there?` resolve `it/there` from the most recent place discussed in chat before calling the weather tool.
- Weather-context corrections such as `서울이 아니라... 난 지금 퀸즈야` and `I'm in Queens, not Seoul` are treated as weather requests for the corrected location before web search routing.
- If deterministic cleanup/alias lookup cannot produce a geocodable location, the app sends only the extracted location name to a weather location normalizer, asks for one English place name, and retries Open-Meteo once.
- Conversation summary prompts such as `summarize our conversation`, `sum up what we talked about`, and `이 대화 요약해줘` call the summary sub-agent directly.
- External text summary prompts such as `summarize this article` remain normal chat requests instead of being treated as saved-conversation summaries.

### 5.6 Documentation And Reporting

| ID | Requirement | Status |
| --- | --- | --- |
| DOC-1 | Keep README easy to understand for build, usage, and development history. | Done |
| DOC-2 | Keep README formatting neat and scannable. | Done |
| DOC-3 | Keep one Korean Markdown daily progress report for all working days. | Done |
| DOC-4 | Summarize each working day in five bullet points at maximum. | Done |
| DOC-5 | Include weekday in daily report headings. | Done |

## 6. Non-Functional Requirements

- Keep the implementation simple and modular.
- Keep `src/main.rs` focused on entry point and command dispatch.
- Prefer small modules for feature behavior:
  - `src/cli.rs` for CLI definitions, defaults, and runtime ids.
  - `src/tui.rs` and `src/tui/` for TUI behavior.
  - `src/agent_workflow.rs` and `src/agent_workflow/` for automatic routing and prompt enrichment.
  - `src/llm.rs` for Rig model calls.
  - `src/lib.rs` for Valkey-backed short term memory.
  - `src/long_term_memory.rs` for pgvector Markdown memory.
  - `src/weather.rs` for weather lookup.
  - `src/web_search.rs` for Brave Search API integration.
- Avoid speculative abstractions until repeated behavior needs them.
- Keep sensitive local settings in `.env`, not in git.

## 7. Follow-Up Checklist

- [x] Keep README usage examples aligned with the real CLI/TUI behavior.
- [x] Keep the daily progress report updated after meaningful development days.
- [x] Keep this PRD updated when a requirement changes from planned to done.
- [ ] Add new requirements here before implementing larger behavior changes.
- [x] Re-check the architecture diagram when modules move or new tools are added.

## 8. Validation Commands

Use these checks after implementation changes:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
git diff --check
```

## 9. Readme

- Include
  - visuals that help understand the repo
  - how to use
  - how to install
  - architecture(agent, tools, solutions)
