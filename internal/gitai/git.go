package gitai

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

type repoContext struct {
	repoRoot             string
	repoName             string
	branchName           string
	diffStat             string
	diffPatch            string
	diffTruncated        bool
	diffStatTruncated    bool
	changedFileCount     int
	representedFileCount int
}

func collectRepoContext(ctx context.Context, cfg config) (repoContext, error) {
	repoRoot := strings.TrimSpace(os.Getenv("GIT_AI_COMMIT_REPO_ROOT"))
	if repoRoot == "" {
		value, err := runGit(ctx, "", "rev-parse", "--show-toplevel")
		if err != nil {
			return repoContext{}, err
		}
		repoRoot = strings.TrimSpace(value)
	}

	branchName, err := currentBranch(ctx, repoRoot)
	if err != nil {
		return repoContext{}, err
	}

	diffStat, err := runGit(ctx, repoRoot, "diff", "--cached", "--stat", "--no-ext-diff")
	if err != nil {
		return repoContext{}, err
	}

	diffPatch, err := runGit(ctx, repoRoot, "diff", "--cached", "--no-ext-diff", "--unified=3")
	if err != nil {
		return repoContext{}, err
	}

	diffStat, diffPatch, sampling := prepareDiffForPrompt(diffStat, diffPatch, cfg.maxDiffBytes)
	if sampling.sampled {
		fmt.Fprintf(
			os.Stderr,
			"git-ai-commit: staged diff selectively sampled within %d byte budget (%d/%d files represented)\n",
			cfg.maxDiffBytes,
			sampling.representedFiles,
			sampling.totalFiles,
		)
	}

	return repoContext{
		repoRoot:             repoRoot,
		repoName:             filepath.Base(repoRoot),
		branchName:           branchName,
		diffStat:             strings.TrimSpace(diffStat),
		diffPatch:            strings.TrimSpace(diffPatch),
		diffTruncated:        sampling.sampled,
		diffStatTruncated:    sampling.statTruncated,
		changedFileCount:     sampling.totalFiles,
		representedFileCount: sampling.representedFiles,
	}, nil
}

func currentBranch(ctx context.Context, repoRoot string) (string, error) {
	branchName, err := runGit(ctx, repoRoot, "symbolic-ref", "--quiet", "--short", "HEAD")
	if err == nil {
		return strings.TrimSpace(branchName), nil
	}

	branchName, err = runGit(ctx, repoRoot, "rev-parse", "--short", "HEAD")
	if err != nil {
		return "", err
	}

	return "detached-" + strings.TrimSpace(branchName), nil
}

func runGit(ctx context.Context, repoRoot string, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, "git", args...)
	cmd.Env = os.Environ()
	if repoRoot != "" {
		cmd.Dir = repoRoot
	}

	output, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("git %s failed: %w: %s", strings.Join(args, " "), err, strings.TrimSpace(string(output)))
	}
	return string(output), nil
}
