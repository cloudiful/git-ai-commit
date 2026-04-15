use crate::git::run_git;
use crate::prompt::git_config_global_set;
use std::path::Path;

const CAI_ALIAS_VALUE: &str = r#"!f() { git ai-commit "$@"; }; f"#;

pub(super) fn run_init_alias(args: &[String]) -> Result<(), String> {
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            "--force" => force = true,
            other => return Err(format!("unknown init-alias flag: {other}")),
        }
    }

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
