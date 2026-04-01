package main

import (
	"context"
	"fmt"
	"os"
	"time"
)

type generationMetrics struct {
	apiDuration time.Duration
}

func runGenerate() error {
	startedAt := time.Now()

	cfg, err := loadConfigForInteractiveUse()
	if err != nil {
		return err
	}

	ctx, cancel := context.WithTimeout(context.Background(), cfg.timeout)
	defer cancel()

	repoCtx, err := collectRepoContext(ctx, cfg)
	if err != nil {
		return err
	}

	message, metrics, err := generateMessage(ctx, cfg, repoCtx)
	if err != nil {
		return err
	}

	if _, err := fmt.Fprintln(os.Stdout, message); err != nil {
		return err
	}

	logTiming(cfg, "generate", startedAt, metrics)
	return nil
}

func logTiming(cfg config, mode string, startedAt time.Time, metrics generationMetrics) {
	if !cfg.showTiming {
		return
	}

	total := time.Since(startedAt)
	fmt.Fprintf(os.Stderr, "git-ai-commit: %s completed in %s (api %s)\n", mode, total.Round(time.Millisecond), metrics.apiDuration.Round(time.Millisecond))
}
