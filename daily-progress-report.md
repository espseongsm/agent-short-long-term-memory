# 개발 진행 보고서

이 파일은 모든 작업일의 개발 진행 상황을 하나의 Markdown 보고서로 관리합니다.

## 2026-05-06 (수요일)

- 사용자 ID, 세션 ID, 타임스탬프, 역할, 내용을 포함한 Valkey 기반 단기 채팅 기록 저장을 구현했다.
- 기본 모델 `Qwen/Qwen3.6-35B-A3B`를 사용하는 OpenAI 호환 모델 호출에 Rig 워크플로 메시지 구성을 연결했다.
- 영어/한국어 입력, Markdown 대화 렌더링, 즉시 사용자 메시지 표시, 키보드/마우스 스크롤을 지원하는 Ratatui TUI를 구축했다.
- 오래 걸리는 모델 응답 중 TUI 상태와 대화창 안 LLM 활동 보존을 추가하고, 날씨/웹 검색 도구와 요청 증폭 서브에이전트를 자동 라우팅과 CLI/TUI 명령에 연결했다.
- `Ctrl+C`/`Ctrl+V` 캡처와 붙여넣기 줄바꿈 정리, TUI 내부 reasoning effort 제어, pgvector 기반 로컬 Markdown 장기 메모리 색인/검색을 추가하고 문서를 갱신했다.

## 2026-05-07 (목요일)

- PRD의 코딩 컨벤션을 반영해 CLI 자동 프롬프트 보강 흐름을 `main.rs`에서 `agent_workflow.rs`로 옮기고, `main.rs`는 진입점과 명령 분기에 집중하도록 정리했다.
- 모델 호출에서 `async-openai` 의존성을 제거하고 Rig의 OpenAI 호환 provider로 직접 요청을 보내도록 바꿨다.
- DuckDuckGo 즉답 기반 검색을 Brave Search API 기반 전체 웹 검색으로 교체하고, `.env`의 `BRAVE_SEARCH_API_KEY`/`WEB_SEARCH_API_KEY`를 사용하도록 했다.
- README에 코드 레이아웃을 추가해 빌드/사용 설명과 함께 주요 모듈의 역할을 빠르게 파악할 수 있게 했다.
- PRD 적용 후 형식 검사, 단위 테스트, Clippy, CLI 도움말 스모크 체크로 동작을 확인했다.

### 검증

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

### 아키텍처 스냅샷

```text
TUI / chat CLI
 |
 |-- prompt/system.yaml
 |-- live pending-response status
 |-- kept in-conversation LLM activity
 |-- automatic current weather routing + CLI command
 |-- automatic web search routing + CLI/TUI command
 |-- automatic request amplifier routing + CLI/TUI command
 |-- Ctrl+C / Ctrl+V clipboard capture
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
