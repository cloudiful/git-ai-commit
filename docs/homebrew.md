# Homebrew distribution

`git-ai-commit` installs from the [cloudiful/tap](https://github.com/cloudiful/homebrew-tap)
tap as a binary formula.

```sh
brew tap cloudiful/tap
brew install cloudiful/tap/git-ai-commit
```

Supported platforms are Apple Silicon macOS, Linux ARM64, and Linux x86_64.
Intel Macs are not supported because no Intel macOS build is published.

Upgrade with:

```sh
brew upgrade cloudiful/tap/git-ai-commit
```

## Formula generation

The source of truth for the formula is
[`.github/scripts/homebrew/formula.rb.tmpl`](../.github/scripts/homebrew/formula.rb.tmpl).
[`generate_formula.py`](../.github/scripts/homebrew/generate_formula.py) reads
GitHub release metadata and renders the complete formula from it:

- The tag must be stable `vMAJOR.MINOR.PATCH` and match the release tag.
- The release must be published, not a draft, and not a prerelease.
- The three assets `git-ai-commit-macos-arm64`,
  `git-ai-commit-linux-arm64`, and `git-ai-commit-linux-amd64` must be uploaded
  exactly once each.
- Each asset must carry a `sha256:<64 hex>` digest and the expected
  `cloudiful/git-ai-commit` download URL.

The script validates every input before writing and only writes
`Formula/git-ai-commit.rb` in a supplied tap checkout. It never runs git. An
older release cannot downgrade the tap, and re-rendering an existing version
with the same content is a no-op.

Preview the formula for a tag without a tap checkout:

```sh
uv run --no-project python .github/scripts/homebrew/generate_formula.py \
  --tag v0.1.17 --dry-run
```

Render into a tap checkout:

```sh
uv run --no-project python .github/scripts/homebrew/generate_formula.py \
  --tag v0.1.17 --tap /path/to/homebrew-tap
```

The release workflow publishes GitHub asset digests, so the formula does not
read `.sha256` sidecar files. Releases no longer generate or upload those
sidecars.

## Automatic updates

[`bump-homebrew-tap.yml`](../.github/workflows/bump-homebrew-tap.yml) runs after
the `Release` workflow completes successfully for a tag push in this repository.
It resolves the stable tag from the release run's head commit, checks the tag
against the repository ref, and updates only `Formula/git-ai-commit.rb` in
`cloudiful/homebrew-tap`.

A single concurrency group serializes tap updates. The workflow fetches the
current tap `main` before rendering, and commits and pushes only when the
formula changed.

## Token configuration

Add a `HOMEBREW_TAP_TOKEN` secret to this repository's Actions secrets. Use a
fine-grained personal access token:

- Repository access: `cloudiful/homebrew-tap`.
- Contents: read and write.

The token is used only to check out, commit, and push the tap. The source
repository uses the workflow's read-only `GITHUB_TOKEN`.

A missing `HOMEBREW_TAP_TOKEN` fails the update job before the tap is checked
out. A manual dry run does not require the token.

## Manual dispatch and backfill

Run the workflow from the Actions UI or the GitHub CLI:

```sh
gh workflow run bump-homebrew-tap.yml -f tag=v0.1.17
```

Render without touching the tap:

```sh
gh workflow run bump-homebrew-tap.yml -f tag=v0.1.17 -f dry_run=true
```

Manual runs accept any stable tag whose release is published; they do not have
an originating release run to match.

## Validation limits

- Tap CI runs `brew audit`, `brew install --build-from-source`, `brew test`,
  and `brew livecheck` for both tap formulae on `macos-14` and
  `ubuntu-latest`.
- The local generator tests are offline and use fixture release metadata:
  `uv run --no-project python -m unittest discover -s .github/scripts/homebrew -p 'test_*.py' -v`.
- Homebrew is not available in every development environment, so formula
  install and livecheck results come from tap CI, not local runs.
- Linux ARM64 assets cannot be installed or tested locally on an x86_64 host.
- The macOS deployment target of the published binary is not verified by these
  checks.
