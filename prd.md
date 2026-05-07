# Agent short and long term memory

## Multi-agent workflow

- Use Rig's OpenAI-compatible provider for model calls.
- Use Rig as a main framework for agent workflow such as overall process, prompts and agent communication
- Default model is `Qwen/Qwen3.6-35B-A3B`.
- Chat history is saved in Valkey.
- tui is the interface of the agent. This tui supports English and Korean.
- conversation pane in tui can be scrolled by keyboard and mouse to check the chat history in a session. And conversation pane supports markdown.
- URL and key are loaded from `.env`.
- model is Qwen/Qwen3.6-35B-A3B by default.
- user message shows up in the conversation pane as soon as a user typed in.
- system prompt for agent is saved in prompt folder in a yaml format.
- what llm does should be shown in tui because response often takes time. And the user will be bored. Please shows llm's actions in the middle of conversation between user and assistant. Keep the llm's action in the conversation pane.
- control reasoning effort of llm for latency. It can be controlled in tui.
- add current weather tool, which is automatically called for weather questions
- add full web search API tool, which is automatically called
- user request amplifier: this is an sub-agent that amplifies the user's request when the request is too simple to answer correctly, which is automatically called.
- tui supports captures such as ctrl+c and ctrl+v

## Short term memory

- Use open source Valkey as a memory database. Valkey is a fork of Redis, supported by the Linux Foundation.
- Documentation is [here](https://valkey.io/topics/).
- Use Valkey as short term memory to save chat history and support chat history search.
- chat history is saved with user id, session id, timestamp
- whenever cargo run, re-new user is, session id.

## Long term memory

- Long term memory is implemented using pg-vector.
- data source is md files in this local machine.

## Development progress daily report

- create a daily progress report(Korean) in markdown format such as what we have developed in a day.
- One file contains all working days's progress.
- Daily progress is summed up in five bullet points at maximum

## README

- easy to understand this repo in terms of how to build, use, and have developed.
- keep format neat.

## Coding convension

- modulize as much as easy to understand.
- main.rs is the entry point of this repo.
