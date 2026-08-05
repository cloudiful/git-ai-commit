use crate::commit::{run_commit, run_doctor};
use crate::generate::run_generate;
use clap::{Args, CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "git-ai-commit", version, disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate,
    Doctor,
    Commit(CommitArgs),
}

#[derive(Args)]
#[command(trailing_var_arg = true)]
struct CommitArgs {
    #[arg(value_name = "GIT_COMMIT_ARGS", num_args = 0.., allow_hyphen_values = true)]
    args: Vec<String>,
}

pub async fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args[0].starts_with('-') {
        match args.first().map(String::as_str) {
            Some("-V" | "--version") => {
                print!("{}", Cli::command().render_version());
                return Ok(());
            }
            Some("-h" | "--help") => {
                Cli::command().print_help().map_err(|err| err.to_string())?;
                println!();
                return Ok(());
            }
            _ => return run_commit(&args).await,
        }
    }

    let cli = Cli::try_parse_from(
        std::iter::once("git-ai-commit").chain(args.iter().map(String::as_str)),
    )
    .map_err(|err| err.to_string())?;

    match cli.command {
        Commands::Generate => run_generate().await,
        Commands::Doctor => run_doctor(&[]).await,
        Commands::Commit(CommitArgs { args }) => run_commit(&args).await,
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn rejects_unknown_subcommand() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let err = rt
            .block_on(run(vec!["wat".to_string()]))
            .expect_err("expected usage error");
        assert!(err.contains("Usage:"));
        assert!(err.contains("git-ai-commit"));
    }

    #[test]
    fn forwards_leading_flags_to_commit_mode() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let err = rt
            .block_on(run(vec!["--edit".to_string()]))
            .expect_err("expected commit parse error");
        assert!(err.contains("unknown git-ai-commit flag"));
    }

    #[test]
    fn prints_version() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(run(vec!["--version".to_string()]))
            .expect("version should print");
        rt.block_on(run(vec!["-V".to_string()]))
            .expect("version should print");
    }

    #[test]
    fn prints_help() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(run(vec!["--help".to_string()]))
            .expect("help should print");
        rt.block_on(run(vec!["-h".to_string()]))
            .expect("help should print");
    }
}
