# 개발 진행 보고서

이 파일은 모든 작업일의 개발 진행 상황을 하나의 Markdown 보고서로 관리합니다.

## 2026-05-06 (수요일)

- 사용자 ID, 세션 ID, 타임스탬프, 역할, 내용을 포함한 Valkey 기반 단기 채팅 기록 저장을 구현했다.
- 기본 모델 `gpt-5.5`를 사용하는 OpenAI 호환 모델 호출에 Rig 워크플로 메시지 구성을 연결했다.
- 영어/한국어 입력, Markdown 대화 렌더링, 즉시 사용자 메시지 표시, 키보드/마우스 스크롤을 지원하는 Ratatui TUI를 구축했다.
- 오래 걸리는 모델 응답 중 TUI 상태와 대화창 안 LLM 활동 보존을 추가하고, 날씨/웹 검색 도구와 요청 증폭 서브에이전트를 자동 라우팅과 CLI/TUI 명령에 연결했다.
- `Ctrl+C`/`Ctrl+V` 캡처와 붙여넣기 줄바꿈 정리, TUI 내부 reasoning effort 제어, pgvector 기반 로컬 Markdown 장기 메모리 색인/검색을 추가하고 문서를 갱신했다.

## 2026-05-07 (목요일)

- PRD의 코딩 컨벤션을 반영해 CLI 자동 프롬프트 보강 흐름을 `main.rs`에서 `agent_workflow.rs`로 옮기고, `main.rs`는 진입점과 명령 분기에 집중하도록 정리했다.
- 모델 호출에서 `async-openai` 의존성을 제거하고 Rig의 OpenAI 호환 provider로 직접 요청을 보내도록 바꿨다.
- DuckDuckGo 즉답 기반 검색을 Brave Search API 기반 전체 웹 검색으로 교체하고, `.env`의 `BRAVE_SEARCH_API_KEY`/`WEB_SEARCH_API_KEY`를 사용하도록 했다.
- README에 코드 레이아웃을 추가해 빌드/사용 설명과 함께 주요 모듈의 역할을 빠르게 파악할 수 있게 했다.
- PRD와 기본 모델을 맞추고 TUI 마우스 드래그 대화 선택 복사, 소수점 경과 시간 표시와 세션 token usage 표시, 한국어/외국 날씨 위치 정규화와 LLM fallback, `it/there` 날씨 후속 질문 해석, 자동 대화 요약 서브에이전트, `prompt/` YAML 기반 agent prompt를 추가했으며, 긴 `main`/`tui`/`agent_workflow` 소스를 책임별 모듈로 분리했다.

## 2026-05-11 (월요일)

- `README 2.md`와 기존 README를 통합해 대문자 `README.md` 하나로 정리하고, Mermaid 런타임 흐름/모듈 맵/날씨 fallback 다이어그램을 포함한 최신 프로젝트 문서로 재구성했다.
- pgvector 장기 메모리를 기본 로컬 URL로 설정하고, 일반 TUI/chat 시작 전에 pgvector 연결과 스키마 초기화를 검증해 서비스가 꺼져 있으면 명확히 실패하도록 했다.
- `long-term-index`의 기본 Markdown 경로를 로컬 Obsidian/iCloud 메모리 위치로 맞추고, pgvector ranking을 보강하되 일반 질문이 아닌 long-term memory/메모/노트/Markdown 요청에서만 검색하도록 자동 라우팅을 좁혔다.
- Valkey에 저장된 chat session과 session-backed model token usage event를 SCAN으로 훑어 CLI와 `dashboard-server`에서 저장 대화와 토큰 사용량을 UTC 날짜/세션별로 보고, 웹에서는 날짜/세션/유저 header 우선순위를 바꿀 수 있게 했다.
- `agent_workflow/actions`, `weather_locations`, `tui/render`를 책임별 하위 모듈로 더 나누고, `dashboard_timestamp` 중복 helper와 AGENTS.md 오타/빈 bullet, `hello what's the weather today?` 날씨 위치 추출 회귀를 정리했다.

### 검증

```sh
cargo fmt --check
CARGO_TARGET_DIR=target/codex-rig-search cargo test
CARGO_TARGET_DIR=target/codex-rig-search cargo clippy -- -D warnings
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- --help
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- tui --help
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- dashboard --help
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- dashboard-server --help
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- web-search --help
CARGO_TARGET_DIR=target/codex-rig-search cargo run --quiet -- summary --help
cargo run --quiet -- dashboard --limit 5 --preview-chars 80
cargo run --quiet -- dashboard-server --port 7879
cargo run --quiet -- weather "hello what's the weather today?" --reasoning-effort low
cargo run --quiet -- weather "치앙마이 날씨는?" --reasoning-effort low
cargo run --quiet -- chat "치앙마이 날씨는?" --reasoning-effort low
cargo tree | rg "async-openai|rig-core"
git diff --check
```

### 아키텍처 스냅샷

```text
TUI / chat CLI
 |
 |-- cli definitions/defaults/runtime ids
 |-- prompt/*.yaml agent prompts
 |-- prompt/system.yaml generated archive
 |-- live pending-response status
 |-- kept in-conversation LLM activity
 |-- session token usage display
 |-- automatic summary agent + CLI/TUI command
 |-- automatic current weather routing + CLI command
 |   |-- cleanup/alias -> Open-Meteo -> location normalizer -> Open-Meteo retry
 |-- automatic web search routing + CLI/TUI command
 |-- automatic request amplifier routing + CLI/TUI command
 |-- Ctrl+C / Ctrl+V / mouse selection clipboard capture
 |-- optional reasoning effort control
 |-- Markdown conversation rendering
 |-- keyboard and mouse conversation scrolling
 |
 v
Rig workflow messages
 |
 v
Rig OpenAI-compatible provider
 |
 v
Model response
 |
 v
Valkey chat history
 |
 |-- history
 |-- search
 |-- remember / recall / forget
 |
 v
pgvector long term memory
 |
 |-- local Markdown indexing
 |-- hashed vector search
 |-- optional automatic chat context
```
