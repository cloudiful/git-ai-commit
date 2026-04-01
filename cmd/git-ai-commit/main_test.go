package main

import (
	"reflect"
	"testing"
)

func TestRunDefaultsToCommit(t *testing.T) {
	restoreCommit := runCommitFn
	t.Cleanup(func() {
		runCommitFn = restoreCommit
	})

	called := false
	runCommitFn = func(args []string) error {
		called = true
		if args != nil {
			t.Fatalf("expected nil args, got %v", args)
		}
		return nil
	}

	if err := run(nil); err != nil {
		t.Fatalf("run returned error: %v", err)
	}
	if !called {
		t.Fatal("expected runCommitFn to be called")
	}
}

func TestRunCommitSubcommandUsesCommitFlow(t *testing.T) {
	restoreCommit := runCommitFn
	t.Cleanup(func() {
		runCommitFn = restoreCommit
	})

	var gotArgs []string
	runCommitFn = func(args []string) error {
		gotArgs = append([]string(nil), args...)
		return nil
	}

	if err := run([]string{"commit", "-s"}); err != nil {
		t.Fatalf("run returned error: %v", err)
	}

	wantArgs := []string{"-s"}
	if !reflect.DeepEqual(gotArgs, wantArgs) {
		t.Fatalf("runCommitFn args = %v, want %v", gotArgs, wantArgs)
	}
}

func TestRunOptionArgsUseCommitFlow(t *testing.T) {
	restoreCommit := runCommitFn
	t.Cleanup(func() {
		runCommitFn = restoreCommit
	})

	var gotArgs []string
	runCommitFn = func(args []string) error {
		gotArgs = append([]string(nil), args...)
		return nil
	}

	if err := run([]string{"-s"}); err != nil {
		t.Fatalf("run returned error: %v", err)
	}

	wantArgs := []string{"-s"}
	if !reflect.DeepEqual(gotArgs, wantArgs) {
		t.Fatalf("runCommitFn args = %v, want %v", gotArgs, wantArgs)
	}
}
