use crate::config::DiffBudgetConfig;
use crate::openai::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt_scaffold};
use crate::tokenizer::Tokenizer;

const DIFF_STAT_SAFETY_CAP_TOKENS: usize = 2048;
const DIFF_PROMPT_SAFETY_RESERVE_TOKENS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DiffBudget {
    pub(crate) configured_tokens: usize,
    pub(crate) effective_tokens: usize,
}

pub(crate) fn resolve_diff_budget(
    config: DiffBudgetConfig,
    repo_name: &str,
    branch_name: &str,
    changed_file_count: usize,
) -> Result<DiffBudget, String> {
    let Some(model_context_tokens) = config.model_context_tokens else {
        return Ok(DiffBudget {
            configured_tokens: config.max_tokens,
            effective_tokens: config.max_tokens,
        });
    };

    let tokenizer = Tokenizer::new()?;
    let prompt_scaffold = build_prompt_scaffold(repo_name, branch_name, changed_file_count);
    let non_diff_prompt_tokens = tokenizer.count(SYSTEM_PROMPT) + tokenizer.count(&prompt_scaffold);
    let context_available_for_diff = model_context_tokens
        .saturating_sub(non_diff_prompt_tokens)
        .saturating_sub(MAX_OUTPUT_TOKENS)
        .saturating_sub(DIFF_PROMPT_SAFETY_RESERVE_TOKENS);

    Ok(DiffBudget {
        configured_tokens: config.max_tokens,
        effective_tokens: config.max_tokens.min(context_available_for_diff),
    })
}

pub(super) fn diff_stat_cap(budget: DiffBudget) -> usize {
    let mut stat_cap = DIFF_STAT_SAFETY_CAP_TOKENS;
    if budget.effective_tokens > 0 && stat_cap > budget.effective_tokens / 3 {
        stat_cap = budget.effective_tokens / 3;
    }
    if stat_cap < 64 {
        stat_cap = 64;
    }
    stat_cap
}

pub(super) fn patch_budget(
    budget: DiffBudget,
    trimmed_stat_len: usize,
    sampling_notice_len: usize,
) -> usize {
    budget
        .effective_tokens
        .max(sampling_notice_len)
        .saturating_sub(trimmed_stat_len)
}

pub(super) fn phase_quota(
    remaining: usize,
    slots: usize,
    min_quota: usize,
    max_quota: usize,
) -> usize {
    if remaining == 0 {
        return 0;
    }
    if slots == 0 {
        return remaining;
    }

    let mut quota = remaining / slots;
    quota = quota.max(min_quota).min(max_quota).min(remaining);
    quota
}
