use crate::commit::{run_commit, run_doctor, run_init_alias};
use crate::generate::run_generate;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "git-ai-commit", disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate,
    InitAlias(InitAliasArgs),
    Doctor,
    Commit(CommitArgs),
}

#[derive(Args)]
struct InitAliasArgs {
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
#[command(trailing_var_arg = true)]
struct CommitArgs {
    #[arg(value_name = "GIT_COMMIT_ARGS", num_args = 0.., allow_hyphen_values = true)]
    args: Vec<String>,
}

pub fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with('-') {
        return run_commit(&args);
    }

    let cli = Cli::try_parse_from(
        std::iter::once("git-ai-commit").chain(args.iter().map(String::as_str)),
    )
    .map_err(|err| err.to_string())?;

    match cli.command {
        Commands::Generate => run_generate(),
        Commands::InitAlias(InitAliasArgs { force }) => {
            let args = if force {
                vec!["--force".to_string()]
            } else {
                Vec::new()
            };
            run_init_alias(&args)
        }
        Commands::Doctor => run_doctor(&[]),
        Commands::Commit(CommitArgs { args }) => run_commit(&args),
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn rejects_unknown_subcommand() {
        let err = run(vec!["wat".to_string()]).expect_err("expected usage error");
        assert!(err.contains("Usage:"));
        assert!(err.contains("git-ai-commit"));
    }

    #[test]
    fn forwards_leading_flags_to_commit_mode() {
        let err = run(vec!["--edit".to_string()]).expect_err("expected commit parse error");
        assert!(err.contains("unknown git-ai-commit flag"));
    }
}
