# git-ai-commit

AI-generated Git commit messages, wired into normal `git commit` flow.

`git-ai-commit` reads your staged changes, redacts sensitive-looking values before sending anything upstream, asks a model for a Conventional Commit-style message, and then commits with Git. If AI should be skipped or a provider is unusable, it falls back to plain `git commit`.

## Features

- Use it as a real Git subcommand: `git ai-commit`
- OpenAI-compatible by default
- Tries `v1/responses` first, then falls back to `v1/chat/completions`
- Supports Ollama and Anthropic-compatible endpoints
- Redacts sensitive-looking values from diffs before sending prompts
- Preserves normal Git behavior for flows like `-m`, `--amend`, and path arguments

## Install

Download a binary from [GitHub Releases](https://github.com/cloudiful/git-ai-commit/releases).

Or install from source:

```sh
cargo install --git https://github.com/cloudiful/git-ai-commit.git git-ai-commit
```

After that, Git will discover it automatically as `git ai-commit`.

## Quick Start

Configure a provider. OpenAI-compatible is the default mode:

```sh
git config --global ai.commit.apiBase https://api.openai.com/v1
git config --global ai.commit.apiKey YOUR_API_KEY
git config --global ai.commit.model gpt-4.1-mini
```

Then commit from staged changes:

```sh
git add .
git ai-commit
```

Useful variants:

```sh
git ai-commit -s
git ai-commit --no-confirm
git-ai-commit generate
git-ai-commit doctor
```

## Providers

### OpenAI-Compatible

This is the default.

```sh
git config --global ai.commit.provider openai-compatible
git config --global ai.commit.apiBase https://api.openai.com/v1
git config --global ai.commit.apiKey YOUR_API_KEY
git config --global ai.commit.model gpt-4.1-mini
```

`ai.commit.apiBase` can be either a base like `https://api.openai.com/v1` or a full endpoint like `.../v1/responses` or `.../v1/chat/completions`. The tool normalizes it and derives sibling endpoints automatically.

### Ollama

Local Ollama:

```sh
git config --global ai.commit.provider ollama
git config --global ai.commit.apiBase http://localhost:11434
git config --global ai.commit.model llama3.2
```

Local Ollama does not require an API key.

### Anthropic-Compatible

```sh
git config --global ai.commit.provider anthropic-compatible
git config --global ai.commit.apiBase https://api.deepseek.com/anthropic
git config --global ai.commit.apiKey YOUR_API_KEY
git config --global ai.commit.model deepseek-chat
```

## Common Options

- `--no-confirm`: commit immediately without the interactive confirm step
- `--show-redactions`: print the redaction preview before sending the prompt
- `--debug-provider`: print provider endpoints and full response payloads to stderr

## Config

Most users only need these keys:

- `ai.commit.provider`
- `ai.commit.apiBase`
- `ai.commit.apiKey`
- `ai.commit.model`
- `ai.commit.confirmCommit`
- `ai.commit.openEditor`
- `ai.commit.redactSecrets`
- `ai.commit.maxDiffTokens`
- `ai.commit.modelContextTokens`

Environment variables can override config, including:

- `GIT_AI_COMMIT_PROVIDER`
- `GIT_AI_COMMIT_API_BASE`
- `GIT_AI_COMMIT_API_KEY`
- `GIT_AI_COMMIT_MODEL`

## Behavior Notes

- AI commit messages are generated from staged changes only
- Messages are requested in English Conventional Commit style
- `responses` is attempted first for OpenAI-compatible providers
- If `responses` is unsupported or returns no usable text, the tool falls back to `chat/completions`
- Some commit forms intentionally bypass AI and go straight to Git, such as `-m`, `--amend`, `--fixup`, `-a`, and path arguments

## Large Diffs

Large staged diffs are sampled instead of being sent in full.

- Default diff token budget: `16000`
- Auto cap for diff token budget: `64000`
- Commit message output budget: `4096`

If `ai.commit.modelContextTokens` is unset and the provider is OpenRouter, the tool can auto-detect model context from `/v1/models` and adjust the diff budget automatically.

## Troubleshooting

Check config and provider visibility:

```sh
git-ai-commit doctor
```

See provider requests and full payloads:

```sh
git ai-commit --debug-provider
```

If a provider completes `responses` with output tokens but no visible text, `git-ai-commit` treats that as unusable and falls back when possible.
