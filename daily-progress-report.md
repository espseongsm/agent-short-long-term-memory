# 개발 진행 보고서

이 파일은 모든 작업일의 개발 진행 상황을 하나의 Markdown 보고서로 관리합니다.

## 2026-05-06

- 사용자 ID, 세션 ID, 타임스탬프, 역할, 내용을 포함한 Valkey 기반 단기 채팅 기록 저장을 구현했다.
- 기본 모델 `Qwen/Qwen3.6-35B-A3B`를 사용하는 OpenAI 호환 모델 호출에 Rig 워크플로 메시지 구성을 연결했다.
- 영어/한국어 입력, Markdown 대화 렌더링, 즉시 사용자 메시지 표시, 키보드/마우스 스크롤을 지원하는 Ratatui TUI를 구축했다.
- 오래 걸리는 모델 응답 중 TUI 상태와 대화창 안 LLM 활동 보존을 추가하고, 날씨/웹 검색 도구와 요청 증폭 서브에이전트를 자동 라우팅과 CLI/TUI 명령에 연결했다.
- `Ctrl+C`/`Ctrl+V` 캡처, TUI 내부 reasoning effort 제어, pgvector 기반 로컬 Markdown 장기 메모리 색인/검색을 추가하고 문서를 갱신했다.

### 검증

```sh
cargo fmt --check
CARGO_TARGET_DIR=target/prd-check cargo test
CARGO_TARGET_DIR=target/prd-check cargo clippy -- -D warnings
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- tui --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- chat --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- weather --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- web-search --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- long-term-index --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- long-term-search --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- amplify --help
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- weather Seoul
CARGO_TARGET_DIR=target/prd-check cargo run --quiet -- web-search "Rust programming language" --limit 2
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
OpenAI-compatible SDK
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
