package main

import (
	"fmt"
	"git-ai-commit/internal/gitai"
	"os"
)

func main() {
	if err := gitai.Run(os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
