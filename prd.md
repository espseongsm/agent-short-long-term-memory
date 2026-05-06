# Agent short and long term memory

## Agent workflow

- Use an OpenAI-compatible SDK for model calls.
- Use Rig as a main framework for agent workflow such as overall process, prompts and agent communication
- Default model is `Qwen/Qwen3.6-35B-A3B`.
- Chat history is saved in Valkey.
- tui is the interface of the agent.
- Rust + Rig is marked as a logo with letters in the TUI at the top and big.
- URL and key are loaded from `.env`.
- model is Qwen/Qwen3.6-35B-A3B by default.
- user message shows up in the conversation pane as soon as a user typed in. ㅎ

## Short term memory

- Use open source Valkey as a memory database. Valkey is a fork of Redis, supported by the Linux Foundation.
- Documentation is [here](https://valkey.io/topics/).
- Use Valkey as short term memory to save chat history and support chat history search.
- chat history is saved with user id, session id, timestamp

## Long term memory

- Long term memory storage and retrieval behavior is to be defined.
