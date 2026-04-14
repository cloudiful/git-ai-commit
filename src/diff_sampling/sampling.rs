use crate::diff_parse::{DiffFile, DiffFileKind};
use crate::message::{trim_to_utf8_bytes, trim_with_notice_at_line_boundary};
use crate::tokenizer::Tokenizer;

use super::budget::{DiffBudget, diff_stat_cap, patch_budget, phase_quota};
use super::notices::{
    DIFF_DELETED_FILE_NOTICE, DIFF_HEADER_TRUNCATED_NOTICE, DIFF_HUNK_TRUNCATED_NOTICE,
    DIFF_SAMPLING_NOTICE, DIFF_STAT_TRUNCATED_NOTICE,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiffSamplingResult {
    pub total_files: usize,
    pub represented_files: usize,
    pub sampled: bool,
    pub stat_truncated: bool,
}

pub fn prepare_diff_for_prompt(
    files: &[DiffFile],
    diff_stat: &str,
    diff_patch: &str,
    budget: DiffBudget,
) -> Result<(String, String, DiffSamplingResult), String> {
    let mut result = DiffSamplingResult {
        total_files: files.len(),
        represented_files: files.len(),
        sampled: false,
        stat_truncated: false,
    };

    let tokenizer = if budget.is_token_mode() {
        Some(Tokenizer::new()?)
    } else {
        None
    };
    let (trimmed_stat, stat_truncated) = trim_text_with_notice(
        diff_stat,
        diff_stat_cap(budget),
        DIFF_STAT_TRUNCATED_NOTICE,
        tokenizer.as_ref(),
    );
    result.stat_truncated = stat_truncated;

    let normalized_patch = diff_patch.replace("\r\n", "\n");
    let (sampled_patch, represented_files, sampled) = sample_diff_patch(
        &files,
        &normalized_patch,
        patch_budget(
            budget,
            text_len(&trimmed_stat, tokenizer.as_ref()),
            text_len(DIFF_SAMPLING_NOTICE, tokenizer.as_ref()),
        ),
        tokenizer.as_ref(),
    );
    result.represented_files = represented_files;
    result.sampled = sampled;

    Ok((trimmed_stat, sampled_patch, result))
}

pub fn sample_diff_patch(
    files: &[DiffFile],
    raw_diff: &str,
    budget: usize,
    tokenizer: Option<&Tokenizer>,
) -> (String, usize, bool) {
    if raw_diff.trim().is_empty() || budget == 0 {
        return minimal_diff_result(raw_diff, budget, tokenizer);
    }
    if text_len(raw_diff, tokenizer) <= budget {
        return (raw_diff.trim().to_string(), files.len(), false);
    }
    if files.is_empty() {
        let (trimmed, _) = trim_text_with_notice(raw_diff, budget, DIFF_SAMPLING_NOTICE, tokenizer);
        return (trimmed.trim().to_string(), 1, true);
    }

    let mut sampled = String::new();
    let mut remaining = budget;
    let mut represented = 0usize;
    let mut header_added = vec![false; files.len()];
    let mut first_hunk_handled = vec![false; files.len()];

    if append_sample(
        &mut sampled,
        &mut remaining,
        DIFF_SAMPLING_NOTICE,
        tokenizer,
    ) == 0
    {
        return minimal_diff_result(raw_diff, budget, tokenizer);
    }

    for (index, file) in files.iter().enumerate() {
        let quota = phase_quota(
            remaining,
            files.len() - index,
            quota_min(tokenizer, 96, 24),
            quota_max(tokenizer, 320, 80),
        );
        let (header_sample, _) = trim_text_with_notice(
            &sampled_file_header(file),
            quota,
            DIFF_HEADER_TRUNCATED_NOTICE,
            tokenizer,
        );
        if append_sample(&mut sampled, &mut remaining, &header_sample, tokenizer) > 0 {
            header_added[index] = true;
            represented += 1;
        }
    }

    for (index, file) in files.iter().enumerate() {
        if !should_sample_file_hunks(file) || remaining == 0 {
            continue;
        }

        let quota = phase_quota(
            remaining,
            count_files_with_pending_first_hunk(files, &first_hunk_handled, index),
            quota_min(tokenizer, 192, 48),
            quota_max(tokenizer, 960, 240),
        );
        let Some(first_hunk) = file.hunks.first() else {
            continue;
        };

        let (hunk_sample, truncated) =
            trim_text_with_notice(first_hunk, quota, DIFF_HUNK_TRUNCATED_NOTICE, tokenizer);
        if append_sample(&mut sampled, &mut remaining, &hunk_sample, tokenizer) > 0 {
            first_hunk_handled[index] = true;
            if !header_added[index] {
                header_added[index] = true;
                represented += 1;
            }
        }
        if !truncated {
            first_hunk_handled[index] = true;
        }
    }

    for (index, file) in files.iter().enumerate() {
        if !should_sample_file_hunks(file) {
            continue;
        }

        let start = if !file.hunks.is_empty() && first_hunk_handled[index] {
            1
        } else {
            0
        };
        for hunk in file.hunks.iter().skip(start) {
            if remaining == 0 {
                break;
            }

            if text_len(hunk, tokenizer) <= remaining {
                append_sample(&mut sampled, &mut remaining, hunk, tokenizer);
                if !header_added[index] {
                    header_added[index] = true;
                    represented += 1;
                }
                continue;
            }

            let (hunk_sample, _) =
                trim_text_with_notice(hunk, remaining, DIFF_HUNK_TRUNCATED_NOTICE, tokenizer);
            if append_sample(&mut sampled, &mut remaining, &hunk_sample, tokenizer) > 0
                && !header_added[index]
            {
                header_added[index] = true;
                represented += 1;
            }
            remaining = 0;
        }
    }

    (sampled.trim().to_string(), represented, true)
}

fn count_files_with_pending_first_hunk(
    files: &[DiffFile],
    first_hunk_handled: &[bool],
    start: usize,
) -> usize {
    let count = files
        .iter()
        .enumerate()
        .skip(start)
        .filter(|(index, file)| should_sample_file_hunks(file) && !first_hunk_handled[*index])
        .count();

    if count == 0 { 1 } else { count }
}

fn sampled_file_header(file: &DiffFile) -> String {
    let header = if file.kind == DiffFileKind::Binary {
        trim_binary_patch_payload(&file.header)
    } else {
        file.header.clone()
    };

    if file.kind == DiffFileKind::Deleted && !file.hunks.is_empty() {
        format!("{header}{DIFF_DELETED_FILE_NOTICE}")
    } else {
        header
    }
}

fn should_sample_file_hunks(file: &DiffFile) -> bool {
    match file.kind {
        DiffFileKind::Deleted
        | DiffFileKind::Binary
        | DiffFileKind::ModeOnly
        | DiffFileKind::SubmoduleOrOtherHeaderOnly => false,
        DiffFileKind::Renamed | DiffFileKind::Copied => !file.hunks.is_empty(),
        DiffFileKind::Modified | DiffFileKind::Added => !file.hunks.is_empty(),
    }
}

fn trim_binary_patch_payload(header: &str) -> String {
    let mut builder = String::new();
    for line in header.replace("\r\n", "\n").split_inclusive('\n') {
        builder.push_str(line);
        if line.trim() == "GIT binary patch" {
            break;
        }
    }
    builder
}

fn minimal_diff_result(
    raw_diff: &str,
    budget: usize,
    tokenizer: Option<&Tokenizer>,
) -> (String, usize, bool) {
    if raw_diff.trim().is_empty() {
        return (String::new(), 0, false);
    }
    if budget == 0 {
        return (DIFF_SAMPLING_NOTICE.trim().to_string(), 0, true);
    }

    let notice = trim_text(DIFF_SAMPLING_NOTICE, budget, tokenizer);
    if notice.trim().is_empty() {
        (String::new(), 0, true)
    } else {
        (notice.trim().to_string(), 0, true)
    }
}

fn trim_text_with_notice(
    input: &str,
    budget: usize,
    notice: &str,
    tokenizer: Option<&Tokenizer>,
) -> (String, bool) {
    match tokenizer {
        Some(tokenizer) => tokenizer.trim_with_notice_at_line_boundary(input, budget, notice),
        None => trim_with_notice_at_line_boundary(input, budget, notice),
    }
}

fn trim_text(input: &str, budget: usize, tokenizer: Option<&Tokenizer>) -> String {
    match tokenizer {
        Some(tokenizer) => tokenizer.trim_to_budget(input, budget),
        None => trim_to_utf8_bytes(input, budget),
    }
}

fn text_len(input: &str, tokenizer: Option<&Tokenizer>) -> usize {
    match tokenizer {
        Some(tokenizer) => tokenizer.count(input),
        None => input.len(),
    }
}

fn append_sample(
    builder: &mut String,
    remaining: &mut usize,
    chunk: &str,
    tokenizer: Option<&Tokenizer>,
) -> usize {
    if *remaining == 0 || chunk.is_empty() {
        return 0;
    }

    let chunk = if text_len(chunk, tokenizer) > *remaining {
        trim_text(chunk, *remaining, tokenizer)
    } else {
        chunk.to_string()
    };

    if chunk.is_empty() {
        return 0;
    }

    let used = text_len(&chunk, tokenizer);
    builder.push_str(&chunk);
    *remaining = remaining.saturating_sub(used);
    used
}

fn quota_min(tokenizer: Option<&Tokenizer>, byte_value: usize, token_value: usize) -> usize {
    if tokenizer.is_some() {
        token_value
    } else {
        byte_value
    }
}

fn quota_max(tokenizer: Option<&Tokenizer>, byte_value: usize, token_value: usize) -> usize {
    if tokenizer.is_some() {
        token_value
    } else {
        byte_value
    }
}
