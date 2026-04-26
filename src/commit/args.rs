#[derive(Debug, PartialEq, Eq)]
pub(super) struct ParsedAiCommitArgs {
    pub(super) forward_args: Vec<String>,
    pub(super) confirm_override: Option<bool>,
    pub(super) show_redactions: bool,
    pub(super) debug_provider: bool,
}

pub(super) fn should_bypass_ai_commit(args: &[String]) -> bool {
    args.iter().any(|arg| match arg.as_str() {
        "--" | "-m" | "-F" | "-C" | "-c" | "--message" | "--file" | "--reuse-message"
        | "--reedit-message" | "--amend" | "-a" | "--all" | "-i" | "--include" | "-o"
        | "--only" | "--fixup" | "--squash" => true,
        _ if arg.starts_with("-m")
            || arg.starts_with("-F")
            || arg.starts_with("-C")
            || arg.starts_with("-c")
            || arg.starts_with("--message=")
            || arg.starts_with("--file=")
            || arg.starts_with("--reuse-message=")
            || arg.starts_with("--reedit-message=")
            || arg.starts_with("--fixup=")
            || arg.starts_with("--squash=") =>
        {
            true
        }
        _ if arg.starts_with('-') => false,
        _ => true,
    })
}

pub(super) fn parse_ai_commit_args(args: &[String]) -> Result<ParsedAiCommitArgs, String> {
    let mut forward_args = Vec::new();
    let mut confirm_override = None;
    let mut show_redactions = false;
    let mut debug_provider = false;
    let mut passthrough_only = false;

    for arg in args {
        if passthrough_only {
            forward_args.push(arg.clone());
            continue;
        }

        match arg.as_str() {
            "--" => {
                passthrough_only = true;
                forward_args.push(arg.clone());
            }
            "--no-confirm" => confirm_override = Some(false),
            "--show-redactions" => show_redactions = true,
            "--debug-provider" => debug_provider = true,
            "--edit" | "--no-edit" => {
                return Err(format!("unknown git-ai-commit flag: {arg}"));
            }
            _ => forward_args.push(arg.clone()),
        }
    }

    Ok(ParsedAiCommitArgs {
        forward_args,
        confirm_override,
        show_redactions,
        debug_provider,
    })
}

pub(super) fn build_ai_commit_args(
    message_file: String,
    open_editor: bool,
    args: &[String],
) -> Vec<String> {
    let mut commit_args = vec!["commit".to_string()];
    if open_editor {
        commit_args.push("-e".to_string());
    }
    commit_args.push("-F".to_string());
    commit_args.push(message_file);
    commit_args.extend(args.iter().cloned());
    commit_args
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedAiCommitArgs, build_ai_commit_args, parse_ai_commit_args, should_bypass_ai_commit,
    };

    #[test]
    fn bypass_rules_match_go_behavior() {
        let cases = vec![
            (vec![], false),
            (vec!["-s"], false),
            (vec!["--no-verify"], false),
            (vec!["--edit"], false),
            (vec!["--no-edit"], false),
            (vec!["-m", "msg"], true),
            (vec!["-mmsg"], true),
            (vec!["--message=msg"], true),
            (vec!["--amend"], true),
            (vec!["--fixup=reword:HEAD"], true),
            (vec!["-a"], true),
            (vec!["README.md"], true),
        ];

        for (args, want) in cases {
            let args = args.into_iter().map(str::to_string).collect::<Vec<_>>();
            assert_eq!(should_bypass_ai_commit(&args), want, "args: {args:?}");
        }
    }

    #[test]
    fn parses_commit_control_flags_without_forwarding_them() {
        let parsed = parse_ai_commit_args(&[
            "--no-confirm".to_string(),
            "--show-redactions".to_string(),
            "-s".to_string(),
        ])
        .expect("expected parsed args");

        assert_eq!(
            parsed,
            ParsedAiCommitArgs {
                forward_args: vec!["-s".to_string()],
                confirm_override: Some(false),
                show_redactions: true,
                debug_provider: false,
            }
        );
    }

    #[test]
    fn parses_commit_control_flags_after_forwarded_git_flags() {
        let parsed = parse_ai_commit_args(&[
            "-s".to_string(),
            "--show-redactions".to_string(),
            "--no-confirm".to_string(),
            "--debug-provider".to_string(),
        ])
        .expect("expected parsed args");

        assert_eq!(
            parsed,
            ParsedAiCommitArgs {
                forward_args: vec!["-s".to_string()],
                confirm_override: Some(false),
                show_redactions: true,
                debug_provider: true,
            }
        );
    }

    #[test]
    fn parses_debug_provider_without_forwarding_it() {
        let parsed = parse_ai_commit_args(&["--debug-provider".to_string(), "-s".to_string()])
            .expect("expected parsed args");

        assert_eq!(
            parsed,
            ParsedAiCommitArgs {
                forward_args: vec!["-s".to_string()],
                confirm_override: None,
                show_redactions: false,
                debug_provider: true,
            }
        );
    }

    #[test]
    fn rejects_legacy_edit_flags() {
        let err = parse_ai_commit_args(&["--edit".to_string()]).expect_err("expected error");
        assert!(err.contains("unknown git-ai-commit flag"));
        assert!(err.contains("--edit"));
    }

    #[test]
    fn stops_parsing_control_flags_after_double_dash() {
        let parsed = parse_ai_commit_args(&[
            "-s".to_string(),
            "--".to_string(),
            "--no-confirm".to_string(),
        ])
        .expect("expected parsed args");

        assert_eq!(
            parsed,
            ParsedAiCommitArgs {
                forward_args: vec![
                    "-s".to_string(),
                    "--".to_string(),
                    "--no-confirm".to_string()
                ],
                confirm_override: None,
                show_redactions: false,
                debug_provider: false,
            }
        );
    }

    #[test]
    fn builds_commit_args_from_table() {
        let cases = vec![
            (
                false,
                vec![
                    "commit".to_string(),
                    "-F".to_string(),
                    "message.txt".to_string(),
                    "-s".to_string(),
                ],
            ),
            (
                true,
                vec![
                    "commit".to_string(),
                    "-e".to_string(),
                    "-F".to_string(),
                    "message.txt".to_string(),
                    "-s".to_string(),
                ],
            ),
        ];

        for (open_editor, expected) in cases {
            let args =
                build_ai_commit_args("message.txt".to_string(), open_editor, &["-s".to_string()]);
            assert_eq!(args, expected);
        }
    }
}
