package gitai

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestShouldBypassAICommit(t *testing.T) {
	testCases := []struct {
		name string
		args []string
		want bool
	}{
		{name: "empty", args: nil, want: false},
		{name: "signoff only", args: []string{"-s"}, want: false},
		{name: "no verify", args: []string{"--no-verify"}, want: false},
		{name: "message flag", args: []string{"-m", "msg"}, want: true},
		{name: "message short joined", args: []string{"-mmsg"}, want: true},
		{name: "message long", args: []string{"--message=msg"}, want: true},
		{name: "amend", args: []string{"--amend"}, want: true},
		{name: "fixup", args: []string{"--fixup=reword:HEAD"}, want: true},
		{name: "all", args: []string{"-a"}, want: true},
		{name: "pathspec", args: []string{"README.md"}, want: true},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			if got := shouldBypassAICommit(tc.args); got != tc.want {
				t.Fatalf("shouldBypassAICommit(%v) = %v, want %v", tc.args, got, tc.want)
			}
		})
	}
}

func TestRunCommitBypassesAIForExplicitMessage(t *testing.T) {
	restoreInteractive := runGitInteractiveFn
	t.Cleanup(func() {
		runGitInteractiveFn = restoreInteractive
	})

	var gotArgs []string
	runGitInteractiveFn = func(_ context.Context, _ string, args ...string) error {
		gotArgs = append([]string(nil), args...)
		return nil
	}

	if err := runCommit([]string{"-m", "hello"}); err != nil {
		t.Fatalf("runCommit returned error: %v", err)
	}

	wantArgs := []string{"commit", "-m", "hello"}
	if !reflect.DeepEqual(gotArgs, wantArgs) {
		t.Fatalf("interactive args = %v, want %v", gotArgs, wantArgs)
	}
}

func TestRunCommitUsesGeneratedMessageFile(t *testing.T) {
	restoreLoad := loadConfigFn
	restoreCollect := collectRepoContextFn
	restoreGenerate := generateMessageFn
	restoreInteractive := runGitInteractiveFn
	t.Cleanup(func() {
		loadConfigFn = restoreLoad
		collectRepoContextFn = restoreCollect
		generateMessageFn = restoreGenerate
		runGitInteractiveFn = restoreInteractive
	})

	loadConfigFn = func() (config, error) {
		return config{timeout: time.Second, showTiming: false}, nil
	}
	collectRepoContextFn = func(_ context.Context, _ config) (repoContext, error) {
		return repoContext{diffStat: " file.txt | 1 +", diffPatch: "diff --git a/file.txt b/file.txt", changedFileCount: 1, representedFileCount: 1}, nil
	}
	generateMessageFn = func(_ context.Context, _ config, _ repoContext) (string, generationMetrics, error) {
		return "feat: add AI commit", generationMetrics{}, nil
	}

	var gotArgs []string
	var gotMessage string
	runGitInteractiveFn = func(_ context.Context, _ string, args ...string) error {
		gotArgs = append([]string(nil), args...)
		messageFile := ""
		for i := 0; i < len(args)-1; i++ {
			if args[i] == "-F" {
				messageFile = args[i+1]
				break
			}
		}
		if messageFile == "" {
			t.Fatal("expected -F message file argument")
		}

		content, err := os.ReadFile(messageFile)
		if err != nil {
			t.Fatalf("read temp message file: %v", err)
		}
		gotMessage = string(content)
		return nil
	}

	if err := runCommit([]string{"-s", "--no-verify"}); err != nil {
		t.Fatalf("runCommit returned error: %v", err)
	}

	if len(gotArgs) < 4 {
		t.Fatalf("expected commit args, got %v", gotArgs)
	}
	if gotArgs[0] != "commit" || gotArgs[1] != "-e" || gotArgs[2] != "-F" {
		t.Fatalf("unexpected commit args prefix: %v", gotArgs)
	}
	if !strings.Contains(gotMessage, "feat: add AI commit\n") {
		t.Fatalf("unexpected message file content: %q", gotMessage)
	}
	if gotArgs[len(gotArgs)-2] != "-s" || gotArgs[len(gotArgs)-1] != "--no-verify" {
		t.Fatalf("expected original commit args to be preserved, got %v", gotArgs)
	}
}

func TestRunCommitShowsGeneratingNoticeBeforeAIRequest(t *testing.T) {
	restoreLoad := loadConfigFn
	restoreCollect := collectRepoContextFn
	restoreGenerate := generateMessageFn
	restoreInteractive := runGitInteractiveFn
	restoreStderr := os.Stderr
	t.Cleanup(func() {
		loadConfigFn = restoreLoad
		collectRepoContextFn = restoreCollect
		generateMessageFn = restoreGenerate
		runGitInteractiveFn = restoreInteractive
		os.Stderr = restoreStderr
	})

	loadConfigFn = func() (config, error) {
		return config{timeout: time.Second, showTiming: false}, nil
	}
	collectRepoContextFn = func(_ context.Context, _ config) (repoContext, error) {
		return repoContext{diffStat: " file.txt | 1 +", diffPatch: "diff --git a/file.txt b/file.txt", changedFileCount: 1, representedFileCount: 1}, nil
	}
	generateMessageFn = func(_ context.Context, _ config, _ repoContext) (string, generationMetrics, error) {
		return "feat: add notice", generationMetrics{}, nil
	}
	runGitInteractiveFn = func(_ context.Context, _ string, args ...string) error {
		return nil
	}

	r, w, err := os.Pipe()
	if err != nil {
		t.Fatalf("create stderr pipe: %v", err)
	}
	os.Stderr = w

	if err := runCommit(nil); err != nil {
		t.Fatalf("runCommit returned error: %v", err)
	}

	if err := w.Close(); err != nil {
		t.Fatalf("close stderr writer: %v", err)
	}

	output, err := io.ReadAll(r)
	if err != nil {
		t.Fatalf("read stderr output: %v", err)
	}

	if !strings.Contains(string(output), "generating commit message from staged changes") {
		t.Fatalf("expected generating notice in stderr, got %q", string(output))
	}
}

func TestRunCommitFallsBackWhenConfigMissing(t *testing.T) {
	restoreLoad := loadConfigFn
	restoreInteractive := runGitInteractiveFn
	restoreIsInteractive := isInteractiveSessionFn
	restorePrompt := promptForMissingConfigFn
	t.Cleanup(func() {
		loadConfigFn = restoreLoad
		runGitInteractiveFn = restoreInteractive
		isInteractiveSessionFn = restoreIsInteractive
		promptForMissingConfigFn = restorePrompt
	})

	loadConfigFn = func() (config, error) {
		return config{}, errors.New("missing config")
	}
	isInteractiveSessionFn = func() bool {
		return false
	}
	promptForMissingConfigFn = func(config) error {
		t.Fatal("did not expect interactive setup prompt")
		return nil
	}

	var gotArgs []string
	runGitInteractiveFn = func(_ context.Context, _ string, args ...string) error {
		gotArgs = append([]string(nil), args...)
		return nil
	}

	if err := runCommit([]string{"-s"}); err != nil {
		t.Fatalf("runCommit returned error: %v", err)
	}

	wantArgs := []string{"commit", "-s"}
	if !reflect.DeepEqual(gotArgs, wantArgs) {
		t.Fatalf("interactive args = %v, want %v", gotArgs, wantArgs)
	}
}

func TestRunCommitPromptsForMissingConfigWhenInteractive(t *testing.T) {
	restoreLoad := loadConfigFn
	restoreCollect := collectRepoContextFn
	restoreGenerate := generateMessageFn
	restoreInteractive := runGitInteractiveFn
	restoreIsInteractive := isInteractiveSessionFn
	restorePrompt := promptForMissingConfigFn
	t.Cleanup(func() {
		loadConfigFn = restoreLoad
		collectRepoContextFn = restoreCollect
		generateMessageFn = restoreGenerate
		runGitInteractiveFn = restoreInteractive
		isInteractiveSessionFn = restoreIsInteractive
		promptForMissingConfigFn = restorePrompt
	})

	loadCalls := 0
	loadConfigFn = func() (config, error) {
		loadCalls++
		if loadCalls == 1 {
			return config{timeout: time.Second, showTiming: false}, &missingConfigError{missingKeys: []string{"ai.commit.apiBase", "ai.commit.apiKey", "ai.commit.model"}}
		}
		return config{
			apiBase:      "https://api.example.com/v1",
			apiKey:       "token",
			model:        "gpt-4.1-mini",
			timeout:      time.Second,
			showTiming:   false,
			maxDiffBytes: defaultMaxDiffBytes,
		}, nil
	}
	isInteractiveSessionFn = func() bool {
		return true
	}

	promptCalled := false
	promptForMissingConfigFn = func(cfg config) error {
		promptCalled = true
		if cfg.timeout != time.Second {
			t.Fatalf("expected partial config to be preserved, got timeout %s", cfg.timeout)
		}
		return nil
	}

	collectRepoContextFn = func(_ context.Context, _ config) (repoContext, error) {
		return repoContext{diffStat: " file.txt | 1 +", diffPatch: "diff --git a/file.txt b/file.txt", changedFileCount: 1, representedFileCount: 1}, nil
	}
	generateMessageFn = func(_ context.Context, _ config, _ repoContext) (string, generationMetrics, error) {
		return "feat: configure interactive setup", generationMetrics{}, nil
	}

	var gotArgs []string
	runGitInteractiveFn = func(_ context.Context, _ string, args ...string) error {
		gotArgs = append([]string(nil), args...)
		return nil
	}

	if err := runCommit([]string{"-s"}); err != nil {
		t.Fatalf("runCommit returned error: %v", err)
	}

	if !promptCalled {
		t.Fatal("expected interactive setup prompt to run")
	}
	if loadCalls != 2 {
		t.Fatalf("loadConfigFn called %d times, want 2", loadCalls)
	}
	if len(gotArgs) < 4 || gotArgs[0] != "commit" || gotArgs[1] != "-e" || gotArgs[2] != "-F" {
		t.Fatalf("expected AI commit flow, got %v", gotArgs)
	}
}

func TestRunCommitSkipsPromptForMissingConfigWhenNotInteractive(t *testing.T) {
	restoreLoad := loadConfigFn
	restoreInteractive := runGitInteractiveFn
	restoreIsInteractive := isInteractiveSessionFn
	restorePrompt := promptForMissingConfigFn
	t.Cleanup(func() {
		loadConfigFn = restoreLoad
		runGitInteractiveFn = restoreInteractive
		isInteractiveSessionFn = restoreIsInteractive
		promptForMissingConfigFn = restorePrompt
	})

	loadConfigFn = func() (config, error) {
		return config{}, &missingConfigError{missingKeys: []string{"ai.commit.apiBase"}}
	}
	isInteractiveSessionFn = func() bool {
		return false
	}

	promptCalled := false
	promptForMissingConfigFn = func(config) error {
		promptCalled = true
		return nil
	}

	var gotArgs []string
	runGitInteractiveFn = func(_ context.Context, _ string, args ...string) error {
		gotArgs = append([]string(nil), args...)
		return nil
	}

	if err := runCommit([]string{"-s"}); err != nil {
		t.Fatalf("runCommit returned error: %v", err)
	}

	if promptCalled {
		t.Fatal("did not expect interactive setup prompt")
	}

	wantArgs := []string{"commit", "-s"}
	if !reflect.DeepEqual(gotArgs, wantArgs) {
		t.Fatalf("interactive args = %v, want %v", gotArgs, wantArgs)
	}
}

func TestRunInitAliasSetsGlobalAlias(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(tempHome, ".config"))

	if err := runInitAlias(nil); err != nil {
		t.Fatalf("runInitAlias returned error: %v", err)
	}

	aliasValue, err := gitConfigGlobalGet("alias.cai")
	if err != nil {
		t.Fatalf("read alias.cai: %v", err)
	}
	if strings.TrimSpace(aliasValue) != caiAliasValue {
		t.Fatalf("alias.cai = %q, want %q", strings.TrimSpace(aliasValue), caiAliasValue)
	}
}

func TestRunInitAliasKeepsExistingAliasWithoutForce(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(tempHome, ".config"))

	if err := gitConfigGlobalSet("alias.cai", "!git commit"); err != nil {
		t.Fatalf("set existing alias: %v", err)
	}

	if err := runInitAlias(nil); err != nil {
		t.Fatalf("runInitAlias returned error: %v", err)
	}

	aliasValue, err := gitConfigGlobalGet("alias.cai")
	if err != nil {
		t.Fatalf("read alias.cai: %v", err)
	}
	if strings.TrimSpace(aliasValue) != "!git commit" {
		t.Fatalf("alias.cai unexpectedly changed to %q", strings.TrimSpace(aliasValue))
	}
}
