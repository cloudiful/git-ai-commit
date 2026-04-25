# git-ai-commit

Small Rust CLI for AI-generated Git commit messages.

## Installation

Download prebuilt binary from [Releases](https://github.com/cloudiful/git-ai-commit/releases).

Or install from git:

```sh
cargo install --git https://github.com/cloudiful/git-ai-commit.git git-ai-commit
```

Then use `git-ai-commit` from your `PATH`.

## Quick Start

Default provider behavior:

- If `ai.commit.provider` is unset, `git-ai-commit` uses `openai-compatible`.

For the default `openai-compatible` mode, configure:

```sh
git config --global ai.commit.apiBase https://your-openai-compatible-endpoint
git config --global ai.commit.apiKey your-token
git config --global ai.commit.model your-model
```

Then use `git ai-commit` instead of `git commit`:

```sh
git add .
git ai-commit
```

Signed commits still work:

```sh
git ai-commit -s
```

## Providers

### OpenAI-Compatible

This is the default when `ai.commit.provider` is not set.

```sh
git config --global ai.commit.provider openai-compatible
git config --global ai.commit.apiBase https://your-openai-compatible-endpoint
git config --global ai.commit.apiKey your-token
git config --global ai.commit.model your-model
```

Use for OpenAI or any compatible endpoint.

### Ollama

Local Ollama uses the OpenAI-compatible API and does not require an API key.

```sh
git config --global ai.commit.provider ollama
git config --global ai.commit.model llama3.2
```

Default `ai.commit.apiBase` for `ollama`:

```sh
git config --global ai.commit.apiBase http://localhost:11434
```

For Ollama cloud:

```sh
git config --global ai.commit.provider ollama
git config --global ai.commit.apiBase https://ollama.com/v1
git config --global ai.commit.apiKey your-ollama-token
git config --global ai.commit.model gpt-oss:20b
```

## How It Works

`git ai-commit` reads staged changes, asks configured model to draft commit
message, asks for `y/e/N` confirmation in interactive use, then runs normal Git
commit flow with generated message.

## Common Options

```sh
git ai-commit --no-confirm
git ai-commit --debug-provider
git ai-commit --show-redactions
git-ai-commit generate
git-ai-commit doctor
```

- `--no-confirm`: skip the interactive `y/e/N` confirmation prompt and commit immediately.
- `--debug-provider`: print provider endpoint, HTTP status, and response body summary to stderr when the upstream request fails.
- `--show-redactions`: print detailed redaction entries; by default only the redaction summary count is shown.
- Interactive confirm prompt: use `y` to commit now, `e` to open the generated message in your editor before committing, or `n`/Enter to cancel.

## Model Context Tokens

`ai.commit.modelContextTokens` lets diff sampling clamp itself against the model's total context window.

```sh
git config --global ai.commit.modelContextTokens 32768
```

When this value is unset and `ai.commit.apiBase` points to OpenRouter, `git-ai-commit` automatically looks up the configured model in OpenRouter's `/v1/models` catalog and uses the returned `top_provider.context_length` or `context_length`.

- Explicit `ai.commit.modelContextTokens` always wins over auto-detection.
- The lookup is done at runtime when generating the prompt, not while reading config.
- Metadata is cached in memory for the current process only.

## Doctor

```sh
git-ai-commit doctor
```

`doctor` prints the resolved provider, base URL, model, and auth mode.

- In `ollama` mode it probes `/v1/models`.
- It also checks whether configured model is visible.

## Environment Overrides

- `GIT_AI_COMMIT_PROVIDER`
- `GIT_AI_COMMIT_API_BASE`
- `GIT_AI_COMMIT_API_KEY`
- `GIT_AI_COMMIT_MODEL`
- `GIT_AI_COMMIT_DEBUG_PROVIDER`
