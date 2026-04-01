package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"time"
)

const caiAliasValue = `!f() { git ai-commit "$@"; }; f`

var (
	loadConfigFn         = loadConfig
	collectRepoContextFn = collectRepoContext
	generateMessageFn    = generateMessage
	runGitInteractiveFn  = runGitInteractive
)

func runCommit(args []string) error {
	if shouldBypassAICommit(args) {
		return runPlainCommit(args)
	}

	startedAt := time.Now()
	cfg, err := loadConfigForInteractiveUse()
	if err != nil {
		return runPlainCommitWithNotice(args, err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), cfg.timeout)
	defer cancel()

	repoCtx, err := collectRepoContextFn(ctx, cfg)
	if err != nil {
		return runPlainCommitWithNotice(args, err)
	}
	if strings.TrimSpace(repoCtx.diffStat) == "" && strings.TrimSpace(repoCtx.diffPatch) == "" {
		return runPlainCommitWithNotice(args, errors.New("no staged changes available for AI prompt"))
	}

	message, metrics, err := generateMessageFn(ctx, cfg, repoCtx)
	if err != nil {
		return runPlainCommitWithNotice(args, err)
	}

	messageFile, err := writeCommitMessageTempFile(message)
	if err != nil {
		return err
	}
	defer os.Remove(messageFile)

	logTiming(cfg, "commit", startedAt, metrics)

	commitArgs := []string{"commit", "-e", "-F", messageFile}
	commitArgs = append(commitArgs, args...)
	return runGitInteractiveFn(context.Background(), "", commitArgs...)
}

func runInitAlias(args []string) error {
	force := false
	for _, arg := range args {
		switch arg {
		case "--force":
			force = true
		default:
			return fmt.Errorf("unknown init-alias flag: %s", arg)
		}
	}

	current, err := gitConfigGlobalGet("alias.cai")
	if err == nil && strings.TrimSpace(current) != "" && strings.TrimSpace(current) != caiAliasValue && !force {
		fmt.Fprintln(os.Stderr, "git-ai-commit: alias.cai already exists; use --force to replace it")
		return nil
	}
	if err == nil && strings.TrimSpace(current) == caiAliasValue {
		fmt.Fprintln(os.Stderr, "git-ai-commit: alias.cai is already configured")
		return nil
	}

	return gitConfigGlobalSet("alias.cai", caiAliasValue)
}

func runDoctor(args []string) error {
	if len(args) > 0 {
		return fmt.Errorf("doctor does not accept arguments")
	}

	cfg, err := loadConfigFn()
	if err != nil {
		fmt.Fprintf(os.Stdout, "config: not ready (%v)\n", err)
	} else {
		fmt.Fprintf(os.Stdout, "config: ready (model %s)\n", cfg.model)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	repoRoot, repoErr := runGit(ctx, "", "rev-parse", "--show-toplevel")
	if repoErr != nil {
		fmt.Fprintf(os.Stdout, "repo: not detected (%v)\n", repoErr)
		return nil
	}

	fmt.Fprintf(os.Stdout, "repo: %s\n", strings.TrimSpace(repoRoot))
	return nil
}

func shouldBypassAICommit(args []string) bool {
	for _, arg := range args {
		switch {
		case arg == "--":
			return true
		case arg == "-m", arg == "-F", arg == "-C", arg == "-c":
			return true
		case arg == "--message", arg == "--file", arg == "--reuse-message", arg == "--reedit-message":
			return true
		case arg == "--amend", arg == "-a", arg == "--all", arg == "-i", arg == "--include", arg == "-o", arg == "--only":
			return true
		case arg == "--fixup", arg == "--squash":
			return true
		case strings.HasPrefix(arg, "-m"), strings.HasPrefix(arg, "-F"), strings.HasPrefix(arg, "-C"), strings.HasPrefix(arg, "-c"):
			return true
		case strings.HasPrefix(arg, "--message="), strings.HasPrefix(arg, "--file="):
			return true
		case strings.HasPrefix(arg, "--reuse-message="), strings.HasPrefix(arg, "--reedit-message="):
			return true
		case strings.HasPrefix(arg, "--fixup="), strings.HasPrefix(arg, "--squash="):
			return true
		case strings.HasPrefix(arg, "-"):
			continue
		default:
			return true
		}
	}

	return false
}

func runPlainCommit(args []string) error {
	commitArgs := append([]string{"commit"}, args...)
	return runGitInteractiveFn(context.Background(), "", commitArgs...)
}

func runPlainCommitWithNotice(args []string, reason error) error {
	if reason != nil {
		fmt.Fprintf(os.Stderr, "git-ai-commit: falling back to plain git commit: %v\n", reason)
	}
	return runPlainCommit(args)
}

func writeCommitMessageTempFile(message string) (string, error) {
	file, err := os.CreateTemp("", "git-ai-commit-*.txt")
	if err != nil {
		return "", err
	}

	defer file.Close()
	if _, err := io.WriteString(file, message+"\n"); err != nil {
		return "", err
	}

	return file.Name(), nil
}

func runGitInteractive(ctx context.Context, repoRoot string, args ...string) error {
	cmd := exec.CommandContext(ctx, "git", args...)
	cmd.Env = os.Environ()
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if repoRoot != "" {
		cmd.Dir = repoRoot
	}
	return cmd.Run()
}

func gitConfigGlobalGet(key string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	return runGit(ctx, "", "config", "--global", "--get", key)
}

func gitConfigGlobalSet(key, value string) error {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	_, err := runGit(ctx, "", "config", "--global", key, value)
	return err
}
