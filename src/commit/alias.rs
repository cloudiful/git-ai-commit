use crate::git::run_git;
use crate::prompt::git_config_global_set;
use clap::Parser;
use std::path::Path;

const CAI_ALIAS_VALUE: &str = r#"!f() { git ai-commit "$@"; }; f"#;

#[derive(Parser)]
#[command(name = "git-ai-commit init-alias", disable_help_flag = true, disable_version_flag = true)]
struct InitAliasCli {
    #[arg(long)]
    force: bool,
}

pub(super) fn run_init_alias(args: &[String]) -> Result<(), String> {
    let parsed = InitAliasCli::try_parse_from(
        std::iter::once("git-ai-commit init-alias").chain(args.iter().map(String::as_str)),
    )
    .map_err(|err| err.to_string())?;
    let force = parsed.force;

    if let Ok(current) = git_config_global_get("alias.cai") {
        let current = current.trim();
        if !current.is_empty() && current != CAI_ALIAS_VALUE && !force {
            eprintln!("git-ai-commit: alias.cai already exists; use --force to replace it");
            return Ok(());
        }
        if current == CAI_ALIAS_VALUE {
            eprintln!("git-ai-commit: alias.cai is already configured");
            return Ok(());
        }
    }

    git_config_global_set("alias.cai", CAI_ALIAS_VALUE)
}

fn git_config_global_get(key: &str) -> Result<String, String> {
    run_git(None::<&Path>, ["config", "--global", "--get", key])
}
