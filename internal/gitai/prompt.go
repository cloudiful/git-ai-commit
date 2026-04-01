package gitai

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
)

var (
	isInteractiveSessionFn   = isInteractiveSession
	promptForMissingConfigFn = promptForMissingConfig
	gitConfigGlobalSetFn     = gitConfigGlobalSet
)

func loadConfigForInteractiveUse() (config, error) {
	cfg, err := loadConfigFn()
	if err == nil {
		return cfg, nil
	}

	var missingErr *missingConfigError
	if !errors.As(err, &missingErr) || !isInteractiveSessionFn() {
		return config{}, err
	}

	fmt.Fprintln(os.Stderr, "git-ai-commit: AI settings are not configured yet.")
	if err := promptForMissingConfigFn(cfg); err != nil {
		return config{}, err
	}

	return loadConfigFn()
}

func promptForMissingConfig(existing config) error {
	reader := bufio.NewReader(os.Stdin)

	type promptField struct {
		current *string
		gitKey  string
		label   string
		hint    string
	}

	fields := []promptField{
		{
			current: &existing.apiBase,
			gitKey:  "ai.commit.apiBase",
			label:   "API base",
			hint:    "Example: https://api.openai.com/v1",
		},
		{
			current: &existing.apiKey,
			gitKey:  "ai.commit.apiKey",
			label:   "API key",
			hint:    "Stored in git config --global ai.commit.apiKey",
		},
		{
			current: &existing.model,
			gitKey:  "ai.commit.model",
			label:   "Model",
			hint:    "Example: gpt-4.1-mini",
		},
	}

	fmt.Fprintln(os.Stderr, "git-ai-commit: press Enter on an empty line to cancel setup.")
	for _, field := range fields {
		if strings.TrimSpace(*field.current) != "" {
			continue
		}

		value, err := promptLine(reader, field.label, field.hint)
		if err != nil {
			return err
		}
		if err := gitConfigGlobalSetFn(field.gitKey, value); err != nil {
			return err
		}
	}

	fmt.Fprintln(os.Stderr, "git-ai-commit: saved required AI settings to global git config.")
	return nil
}

func promptLine(reader *bufio.Reader, label, hint string) (string, error) {
	if hint != "" {
		fmt.Fprintf(os.Stderr, "git-ai-commit: %s\n", hint)
	}
	fmt.Fprintf(os.Stderr, "git-ai-commit: %s: ", label)

	line, err := reader.ReadString('\n')
	if err != nil && !errors.Is(err, io.EOF) {
		if len(line) == 0 {
			return "", err
		}
	}

	value := strings.TrimSpace(line)
	if value == "" {
		return "", errors.New("setup canceled")
	}
	return value, nil
}

func isInteractiveSession() bool {
	return isCharDevice(os.Stdin) && isCharDevice(os.Stdout) && isCharDevice(os.Stderr)
}

func isCharDevice(file *os.File) bool {
	if file == nil {
		return false
	}

	info, err := file.Stat()
	if err != nil {
		return false
	}
	return (info.Mode() & os.ModeCharDevice) != 0
}
