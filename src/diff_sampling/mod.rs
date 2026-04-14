mod budget;
mod notices;
mod sampling;

pub(crate) use budget::{DiffBudget, resolve_diff_budget};
pub(crate) use sampling::prepare_diff_for_prompt;

#[cfg(test)]
pub(crate) use notices::{DIFF_DELETED_FILE_NOTICE, DIFF_SAMPLING_NOTICE};
#[cfg(test)]
pub(crate) use sampling::sample_diff_patch;

#[cfg(test)]
mod tests;
