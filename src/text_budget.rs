pub(crate) fn trim_with_notice_at_line_boundary(
    input: &str,
    max_budget: usize,
    notice: &str,
    measure: impl Fn(&str) -> usize,
    trim_to_budget: impl Fn(&str, usize) -> String,
) -> (String, bool) {
    if max_budget == 0 {
        return (String::new(), !input.is_empty());
    }
    if measure(input) <= max_budget {
        return (input.to_string(), false);
    }

    let available = max_budget.saturating_sub(measure(notice));
    if available == 0 {
        return (trim_to_budget(notice, max_budget), true);
    }

    let mut trimmed = trim_to_budget(input, available);
    if let Some(idx) = trimmed.rfind('\n').filter(|idx| *idx > 0) {
        trimmed.truncate(idx + 1);
    }
    if trimmed.is_empty() {
        trimmed = trim_to_budget(input, available);
    }
    if trimmed.is_empty() {
        return (trim_to_budget(notice, max_budget), true);
    }

    (format!("{trimmed}{notice}"), true)
}
