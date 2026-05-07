use std::time::Instant;

use crate::{
    llm::{LlmResponse, ReasoningEffort, TokenUsage},
    weather::WeatherClient,
    web_search::WebSearchClient,
};
use agent_memory::ChatEntry;
use anyhow::Result;
use tokio::task::JoinHandle;

pub struct TuiConfig {
    pub user_id: String,
    pub session: String,
    pub model: String,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub history_limit: usize,
    pub ttl_seconds: u64,
    pub preamble: String,
    pub amplifier_preamble: String,
    pub summary_preamble: String,
    pub weather_translator_preamble: String,
    pub pgvector_url: Option<String>,
}

pub(super) struct App {
    pub(super) entries: Vec<ChatEntry>,
    pub(super) actions: Vec<AgentAction>,
    pub(super) input: String,
    pub(super) status: String,
    pub(super) token_usage: TokenUsage,
    pub(super) reasoning_effort: Option<ReasoningEffort>,
    pub(super) conversation_scroll: usize,
    pub(super) mouse_selection: Option<MouseSelection>,
    pub(super) pending_response: Option<PendingResponse>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AgentAction {
    pub(super) after_entry_count: usize,
    pub(super) content: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MouseSelection {
    pub(super) start: SelectionPoint,
    pub(super) end: SelectionPoint,
    pub(super) dragged: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct SelectionPoint {
    pub(super) line: usize,
    pub(super) column: usize,
}

impl MouseSelection {
    pub(super) fn is_active(self) -> bool {
        self.dragged || self.start != self.end
    }

    pub(super) fn ordered_points(self) -> (SelectionPoint, SelectionPoint) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

pub(super) struct PendingResponse {
    pub(super) handle: JoinHandle<Result<LlmResponse>>,
    pub(super) started_at: Instant,
    pub(super) action_index: usize,
}

#[derive(Clone, Copy)]
pub(super) struct AgentServices<'a> {
    pub(super) weather: &'a WeatherClient,
    pub(super) web_search: &'a WebSearchClient,
}

impl App {
    pub(super) fn new(entries: Vec<ChatEntry>, reasoning_effort: Option<ReasoningEffort>) -> Self {
        Self {
            entries,
            actions: Vec::new(),
            input: String::new(),
            status: "ready".to_string(),
            token_usage: TokenUsage::default(),
            reasoning_effort,
            conversation_scroll: 0,
            mouse_selection: None,
            pending_response: None,
        }
    }
}
