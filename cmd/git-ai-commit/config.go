package main

import (
	"context"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	defaultTimeoutSec   = 15
	defaultMaxDiffBytes = 60000
)

type config struct {
	apiBase      string
	apiKey       string
	model        string
	showTiming   bool
	useEnvProxy  bool
	timeout      time.Duration
	maxDiffBytes int
}

type missingConfigError struct {
	missingKeys []string
}

func (e *missingConfigError) Error() string {
	return "missing GIT_AI_COMMIT_API_BASE, GIT_AI_COMMIT_API_KEY, or GIT_AI_COMMIT_MODEL"
}

func loadConfig() (config, error) {
	timeoutSec, err := getConfigInt("ai.commit.timeoutSec", "GIT_AI_COMMIT_TIMEOUT_SEC", defaultTimeoutSec)
	if err != nil {
		return config{}, err
	}

	maxDiffBytes, err := getConfigInt("ai.commit.maxDiffBytes", "GIT_AI_COMMIT_MAX_DIFF_BYTES", defaultMaxDiffBytes)
	if err != nil {
		return config{}, err
	}

	cfg := config{
		apiBase:      firstConfigValue("GIT_AI_COMMIT_API_BASE", "ai.commit.apiBase"),
		apiKey:       firstConfigValue("GIT_AI_COMMIT_API_KEY", "ai.commit.apiKey"),
		model:        firstConfigValue("GIT_AI_COMMIT_MODEL", "ai.commit.model"),
		showTiming:   firstConfigBool("GIT_AI_COMMIT_SHOW_TIMING", "ai.commit.showTiming", true),
		useEnvProxy:  firstConfigBool("GIT_AI_COMMIT_USE_ENV_PROXY", "ai.commit.useEnvProxy", false),
		timeout:      time.Duration(timeoutSec) * time.Second,
		maxDiffBytes: maxDiffBytes,
	}

	if missingKeys := missingRequiredConfigKeys(cfg); len(missingKeys) > 0 {
		return cfg, &missingConfigError{missingKeys: missingKeys}
	}

	return cfg, nil
}

func missingRequiredConfigKeys(cfg config) []string {
	var missing []string
	if cfg.apiBase == "" {
		missing = append(missing, "ai.commit.apiBase")
	}
	if cfg.apiKey == "" {
		missing = append(missing, "ai.commit.apiKey")
	}
	if cfg.model == "" {
		missing = append(missing, "ai.commit.model")
	}
	return missing
}

func getConfigInt(configKey, envKey string, fallback int) (int, error) {
	raw := firstConfigValue(envKey, configKey)
	if raw == "" {
		return fallback, nil
	}

	value, err := strconv.Atoi(raw)
	if err != nil || value <= 0 {
		return 0, fmt.Errorf("invalid %s value %q", configKey, raw)
	}
	return value, nil
}

func firstConfigBool(envKey, gitKey string, fallback bool) bool {
	raw := firstConfigValue(envKey, gitKey)
	if raw == "" {
		return fallback
	}

	value, err := strconv.ParseBool(raw)
	if err != nil {
		return fallback
	}
	return value
}

func firstConfigValue(envKey, gitKey string) string {
	if value := strings.TrimSpace(os.Getenv(envKey)); value != "" {
		return value
	}

	value, err := gitConfigGet(gitKey)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(value)
}

func gitConfigGet(key string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	args := []string{"config", "--get", key}
	repoRoot := strings.TrimSpace(os.Getenv("GIT_AI_COMMIT_REPO_ROOT"))
	if repoRoot != "" {
		return runGit(ctx, repoRoot, args...)
	}
	return runGit(ctx, "", args...)
}
