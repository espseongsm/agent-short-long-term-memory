# 개발 진행 보고서

이 파일은 모든 작업일의 개발 진행 상황을 하나의 Markdown 보고서로 관리합니다.

## 2026-05-06

- 사용자 ID, 세션 ID, 타임스탬프, 역할, 내용을 포함한 Valkey 기반 단기 채팅 기록 저장을 구현했다.
- 기본 모델 `Qwen/Qwen3.6-35B-A3B`를 사용하는 OpenAI 호환 모델 호출에 Rig 워크플로 메시지 구성을 연결했다.
- 영어/한국어 입력, Markdown 대화 렌더링, 즉시 사용자 메시지 표시, 키보드/마우스 스크롤을 지원하는 Ratatui TUI를 구축했다.
- 오래 걸리는 모델 응답 중 TUI 상태 표시를 추가하고, 에이전트 시스템 프롬프트를 `prompt/system.yaml`에 저장하도록 유지했다.
- 히스토리, 검색, 메모리 명령, Valkey 설정, GitHub Actions CI, 일일 보고서, 아키텍처, 장기 메모리 보류 사항을 문서화했다.

### 검증

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
VALKEY_URL=redis://127.0.0.1:6379/ cargo test -- --ignored
cargo run -- --help
cargo run -- tui --help
git diff --check
```

### 아키텍처 스냅샷

```text
TUI / chat CLI
 |
 |-- prompt/system.yaml
 |-- live pending-response status
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
```

### 보류

- 장기 메모리는 PRD에서 저장소와 검색 동작이 아직 정의되지 않았으므로 보류했다.
