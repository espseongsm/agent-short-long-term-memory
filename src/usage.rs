use agent_memory::{ShortTermMemory, TokenUsageRecord, TokenUsageSource, TokenUsageTotals};
use anyhow::Result;

use crate::llm::TokenUsage;

pub(crate) fn record_token_usage(
    memory: &mut ShortTermMemory,
    user_id: &str,
    session: &str,
    source: TokenUsageSource,
    usage: TokenUsage,
    ttl_seconds: u64,
) -> Result<()> {
    if !usage.has_usage() {
        return Ok(());
    }

    let record = TokenUsageRecord::for_session(
        user_id,
        session,
        source,
        TokenUsageTotals {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            cache_creation_input_tokens: usage.cache_creation_input_tokens,
        },
    );

    memory.append_token_usage(session, record, ttl_seconds)?;

    Ok(())
}
