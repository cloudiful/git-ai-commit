mod budget;
mod notices;
mod sampling;
mod suppress;

pub(crate) use budget::{DiffBudget, resolve_diff_budget};
pub(crate) use sampling::prepare_diff_for_prompt;
pub(crate) use suppress::{rebuild_patch_from_files, suppress_generated_content};

#[cfg(test)]
pub(crate) use notices::{
    DIFF_DELETED_FILE_NOTICE, DIFF_SAMPLING_NOTICE, DIFF_SUPPRESSED_CONTENT_NOTICE,
};
#[cfg(test)]
pub(crate) use sampling::sample_diff_patch;

#[cfg(test)]
mod tests;
