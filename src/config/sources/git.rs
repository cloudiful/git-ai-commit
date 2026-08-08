use super::super::RawConfigValues;
use super::non_empty_trimmed;
use std::collections::HashMap;
use std::process::Command;

pub(super) fn load_git_values() -> RawConfigValues {
    load_git_values_with(run_git_config_list)
}

fn load_git_values_with(
    run: impl FnOnce() -> std::io::Result<std::process::Output>,
) -> RawConfigValues {
    let Ok(output) = run() else {
        return RawConfigValues::default();
    };
    if !output.status.success() {
        return RawConfigValues::default();
    }

    raw_values_from_map(&parse_git_config_list(&output.stdout))
}

fn run_git_config_list() -> std::io::Result<std::process::Output> {
    let mut command = Command::new("git");
    command.args(["config", "--null", "--list"]);
    if let Ok(repo_root) = std::env::var("GIT_AI_COMMIT_REPO_ROOT")
        && !repo_root.trim().is_empty()
    {
        command.current_dir(repo_root.trim());
    }
    command.output()
}

fn parse_git_config_list(output: &[u8]) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for record in output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let Some(separator) = record.iter().position(|byte| *byte == b'\n') else {
            continue;
        };
        let key = String::from_utf8_lossy(&record[..separator]).to_ascii_lowercase();
        let value = String::from_utf8_lossy(&record[separator + 1..]).into_owned();
        values.insert(key, value);
    }
    values
}

fn raw_values_from_map(values: &HashMap<String, String>) -> RawConfigValues {
    let get = |key: &str| values.get(key).cloned().and_then(non_empty_trimmed);
    RawConfigValues {
        provider: get("ai.commit.provider"),
        api_base: get("ai.commit.apibase"),
        api_key: get("ai.commit.apikey"),
        model: get("ai.commit.model"),
        confirm_commit: get("ai.commit.confirmcommit"),
        open_editor: get("ai.commit.openeditor"),
        enable_fallback: get("ai.commit.enablefallback"),
        redact_secrets: get("ai.commit.redactsecrets"),
        redaction_secret: get("ai.commit.redaction.secret"),
        redaction_domain: get("ai.commit.redaction.domain"),
        redaction_url: get("ai.commit.redaction.url"),
        redaction_email: get("ai.commit.redaction.email"),
        redaction_ip: get("ai.commit.redaction.ip"),
        redaction_cidr: get("ai.commit.redaction.cidr"),
        redaction_phone: get("ai.commit.redaction.phone"),
        redaction_person: get("ai.commit.redaction.person"),
        redaction_organization: get("ai.commit.redaction.organization"),
        show_timing: get("ai.commit.showtiming"),
        use_env_proxy: get("ai.commit.useenvproxy"),
        timeout_sec: get("ai.commit.timeoutsec"),
        max_diff_tokens: get("ai.commit.maxdifftokens"),
        model_context_tokens: get("ai.commit.modelcontexttokens"),
        reasoning_effort: get("ai.commit.reasoningeffort"),
        suppress_diff_dirs: values
            .get("ai.commit.suppressdiffdirs")
            .cloned()
            .map(|value| value.trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{load_git_values_with, parse_git_config_list, raw_values_from_map};

    #[test]
    fn parses_null_records_case_insensitively_and_keeps_last_value() {
        let parsed = parse_git_config_list(
            b"AI.Commit.Model\nfirst\0ai.commit.model\nsecond\0ai.commit.apiKey\n\0",
        );
        let values = raw_values_from_map(&parsed);

        assert_eq!(values.model.as_deref(), Some("second"));
        assert_eq!(values.api_key, None);
    }

    #[test]
    fn preserves_newlines_inside_values() {
        let parsed = parse_git_config_list(b"ai.commit.model\nline one\nline two\0");

        assert_eq!(
            parsed.get("ai.commit.model").map(String::as_str),
            Some("line one\nline two")
        );
    }

    #[test]
    fn ignores_records_without_key_value_separator() {
        let parsed = parse_git_config_list(b"invalid\0ai.commit.model\nvalid\0");

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed.get("ai.commit.model").map(String::as_str),
            Some("valid")
        );
    }

    #[test]
    fn command_failure_returns_empty_git_config() {
        let values = load_git_values_with(|| Err(std::io::Error::other("git unavailable")));

        assert_eq!(values.api_base, None);
        assert_eq!(values.model, None);
    }
}
