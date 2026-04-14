use crate::config::DiffBudgetConfig;
use crate::openai::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt_scaffold};
use crate::tokenizer::Tokenizer;

const DIFF_PROMPT_RESERVE_BYTES: usize = 1024;
const DIFF_STAT_SAFETY_CAP_BYTES: usize = 8192;
const DIFF_STAT_SAFETY_CAP_TOKENS: usize = 2048;
const DIFF_PROMPT_SAFETY_RESERVE_TOKENS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffBudget {
    Bytes {
        max_bytes: usize,
    },
    Tokens {
        configured_tokens: usize,
        effective_tokens: usize,
    },
}

impl DiffBudget {
    pub(crate) fn is_token_mode(&self) -> bool {
        matches!(self, Self::Tokens { .. })
    }
}

pub(crate) fn resolve_diff_budget(
    config: DiffBudgetConfig,
    repo_name: &str,
    branch_name: &str,
    changed_file_count: usize,
) -> Result<DiffBudget, String> {
    match config {
        DiffBudgetConfig::Bytes { max_bytes } => Ok(DiffBudget::Bytes { max_bytes }),
        DiffBudgetConfig::Tokens {
            max_tokens,
            model_context_tokens,
        } => {
            let Some(model_context_tokens) = model_context_tokens else {
                return Ok(DiffBudget::Tokens {
                    configured_tokens: max_tokens,
                    effective_tokens: max_tokens,
                });
            };

            let tokenizer = Tokenizer::new()?;
            let prompt_scaffold = build_prompt_scaffold(repo_name, branch_name, changed_file_count);
            let non_diff_prompt_tokens =
                tokenizer.count(SYSTEM_PROMPT) + tokenizer.count(&prompt_scaffold);
            let context_available_for_diff = model_context_tokens
                .saturating_sub(non_diff_prompt_tokens)
                .saturating_sub(MAX_OUTPUT_TOKENS)
                .saturating_sub(DIFF_PROMPT_SAFETY_RESERVE_TOKENS);

            Ok(DiffBudget::Tokens {
                configured_tokens: max_tokens,
                effective_tokens: max_tokens.min(context_available_for_diff),
            })
        }
    }
}

pub(super) fn diff_stat_cap(budget: DiffBudget) -> usize {
    match budget {
        DiffBudget::Bytes { max_bytes } => {
            let mut stat_cap = DIFF_STAT_SAFETY_CAP_BYTES;
            if max_bytes > 0 && stat_cap > max_bytes / 3 {
                stat_cap = max_bytes / 3;
            }
            if stat_cap < 512 {
                stat_cap = 512;
            }
            stat_cap
        }
        DiffBudget::Tokens {
            effective_tokens, ..
        } => {
            let mut stat_cap = DIFF_STAT_SAFETY_CAP_TOKENS;
            if effective_tokens > 0 && stat_cap > effective_tokens / 3 {
                stat_cap = effective_tokens / 3;
            }
            if stat_cap < 64 {
                stat_cap = 64;
            }
            stat_cap
        }
    }
}

pub(super) fn patch_budget(
    budget: DiffBudget,
    trimmed_stat_len: usize,
    sampling_notice_len: usize,
) -> usize {
    match budget {
        DiffBudget::Bytes { max_bytes } => {
            let mut patch_budget =
                max_bytes.saturating_sub(DIFF_PROMPT_RESERVE_BYTES + trimmed_stat_len);
            if patch_budget < sampling_notice_len {
                patch_budget = sampling_notice_len;
            }
            patch_budget
        }
        DiffBudget::Tokens {
            effective_tokens, ..
        } => effective_tokens.saturating_sub(trimmed_stat_len),
    }
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
