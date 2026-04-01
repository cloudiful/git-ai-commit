# Git AI Commit Message

This repository ships a small Go CLI that drafts commit messages from staged
changes and launches a normal `git commit` flow for you.

The primary entry point is now:

```sh
git ai-commit
```

If you like a shorter command, you can optionally install the `git cai` alias.

## Behavior

- `git ai-commit` generates a commit message from staged changes, writes it to a
  temporary file, and then runs `git commit -e -F <tempfile>`.
- The editor still opens by default, so you can review or tweak the message
  before the commit is finalized.
- Skips AI generation and falls back to plain `git commit` for explicit message
  or rewrite flows such as `-m`, `-F`, `-C`, `-c`, `--amend`, `--fixup`,
  `--squash`, `-a`, `--all`, and path-based commits.
- Fails open. Missing API settings, empty staged diff, or request errors fall
  back to normal Git behavior.
- On a first interactive run, if required AI settings are missing, the command
  prompts for them and saves them into your global Git config before retrying.
- Reads settings from Git config first, with environment variables only used as
  overrides.
- Prints timing to stderr when AI generation actually runs. This is on by
  default and does not affect the commit message content.
- Ignores `HTTP_PROXY` and `HTTPS_PROXY` by default to avoid broken local proxy
  settings blocking commits. Set `ai.commit.useEnvProxy=true` if you explicitly
  want the tool to honor proxy environment variables.

## Install

With Go installed, you can either install into your Go bin directory:

```sh
go install ./cmd/git-ai-commit
```

Or build a local copy inside this config directory:

```sh
go build -o bin/git-ai-commit ./cmd/git-ai-commit
```

If you use `mise`, this repo includes `.mise.toml` for a Go toolchain declaration.
When using `go install`, make sure your shell `PATH` includes `$(go env GOPATH)/bin`
or your configured `GOBIN`. Once the binary is on your `PATH`, Git can invoke it
as `git ai-commit` automatically.

## Usage

Zero-install usage:

```sh
git ai-commit -s
```

Optional short alias:

```sh
git-ai-commit init-alias
git cai -s
```

Other commands:

```sh
git-ai-commit generate
git-ai-commit doctor
git-ai-commit commit -s
```

## Git Config

Recommended:

```sh
git config --global ai.commit.apiBase https://your-openai-compatible-endpoint
git config --global ai.commit.apiKey your-token
git config --global ai.commit.model your-model
```

If those values are missing and you run `git ai-commit` in a normal terminal,
the tool now asks for them interactively and stores them for you.

Optional:

```sh
git config --global ai.commit.timeoutSec 15
git config --global ai.commit.maxDiffBytes 60000
git config --global ai.commit.showTiming true
git config --global ai.commit.useEnvProxy false
```

These values live in your global Git config file, which on this machine is
[config](/Users/cloudiful/.config/git/config).

## Environment Overrides

- `GIT_AI_COMMIT_API_BASE`
- `GIT_AI_COMMIT_API_KEY`
- `GIT_AI_COMMIT_MODEL`

Optional:

- `GIT_AI_COMMIT_TIMEOUT_SEC` defaults to `15`
- `GIT_AI_COMMIT_MAX_DIFF_BYTES` defaults to `60000`
- `GIT_AI_COMMIT_SHOW_TIMING` defaults to `true`
- `GIT_AI_COMMIT_USE_ENV_PROXY` defaults to `false`

## Manual Check

```sh
git config --global ai.commit.apiBase https://example.com
git config --global ai.commit.apiKey token
git config --global ai.commit.model gpt-4.1-mini

git ai-commit
git cai
git-ai-commit generate
```
