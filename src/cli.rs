use crate::commit::{run_commit, run_doctor, run_init_alias};
use crate::generate::run_generate;

pub fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return run_commit(&[]);
    }

    match args[0].as_str() {
        "commit" => run_commit(&args[1..]),
        "generate" => run_generate(),
        "init-alias" => run_init_alias(&args[1..]),
        "doctor" => run_doctor(&args[1..]),
        other if other.starts_with('-') => run_commit(&args),
        _ => Err(usage_error()),
    }
}

fn usage_error() -> String {
    "usage: git-ai-commit [git-commit-args...]\n       git-ai-commit generate\n       git-ai-commit init-alias [--force]\n       git-ai-commit doctor".to_string()
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn rejects_unknown_subcommand() {
        let err = run(vec!["wat".to_string()]).expect_err("expected usage error");
        assert!(err.contains("usage: git-ai-commit"));
    }
}
