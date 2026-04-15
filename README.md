# git-ai-commit

Small Rust CLI for AI-generated Git commit messages.

## Quick Start

Build binary:

```sh
cargo build --release
```

Then make sure `git-ai-commit` is on your `PATH`.

Configure three required values:

```sh
git config --global ai.commit.apiBase https://your-openai-compatible-endpoint
git config --global ai.commit.apiKey your-token
git config --global ai.commit.model your-model
```

After that, replace `git commit` with `git ai-commit`.

```sh
git add .
git ai-commit
```

Signed commit still works:

```sh
git ai-commit -s
```

## How It Works

`git ai-commit` reads staged changes, asks configured model to draft commit
message, asks for `y/e/N` confirmation in interactive use, then runs normal Git
commit flow with generated message.

## Common Options

```sh
git ai-commit --no-confirm
git ai-commit --show-redactions
git-ai-commit generate
git-ai-commit doctor
```

- `--no-confirm`: skip the interactive `y/e/N` confirmation prompt and commit immediately.
- `--show-redactions`: print detailed redaction entries; by default only the redaction summary count is shown.
- Interactive confirm prompt: use `y` to commit now, `e` to open the generated message in your editor before committing, or `n`/Enter to cancel.

## Environment Overrides

- `GIT_AI_COMMIT_API_BASE`
- `GIT_AI_COMMIT_API_KEY`
- `GIT_AI_COMMIT_MODEL`
