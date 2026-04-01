package gitai

import (
	"errors"
	"strings"
)

var (
	runCommitFn    = runCommit
	runGenerateFn  = runGenerate
	runInitAliasFn = runInitAlias
	runDoctorFn    = runDoctor
)

// Run dispatches CLI args to the appropriate subcommand implementation.
func Run(args []string) error {
	return run(args)
}

func run(args []string) error {
	if len(args) == 0 {
		return runCommitFn(nil)
	}

	switch args[0] {
	case "commit":
		return runCommitFn(args[1:])
	case "generate":
		return runGenerateFn()
	case "init-alias":
		return runInitAliasFn(args[1:])
	case "doctor":
		return runDoctorFn(args[1:])
	default:
		if strings.HasPrefix(args[0], "-") {
			return runCommitFn(args)
		}
		return usageError()
	}
}

func usageError() error {
	return errors.New("usage: git-ai-commit [git-commit-args...]\n       git-ai-commit generate\n       git-ai-commit init-alias [--force]\n       git-ai-commit doctor")
}
