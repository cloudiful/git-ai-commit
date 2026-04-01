package main

import (
	"strings"
	"testing"
)

func TestParseDiffFilesSplitsFilesAndHunks(t *testing.T) {
	diff := strings.Join([]string{
		"diff --git a/a.txt b/a.txt",
		"index 111..222 100644",
		"--- a/a.txt",
		"+++ b/a.txt",
		"@@ -1 +1 @@",
		"-old-a",
		"+new-a",
		"@@ -10 +10 @@",
		"-old-a-2",
		"+new-a-2",
		"diff --git a/b.txt b/b.txt",
		"index 333..444 100644",
		"--- a/b.txt",
		"+++ b/b.txt",
		"@@ -1 +1 @@",
		"-old-b",
		"+new-b",
		"",
	}, "\n")

	files := parseDiffFiles(diff)

	if got := len(files); got != 2 {
		t.Fatalf("expected 2 files, got %d", got)
	}
	if got := len(files[0].hunks); got != 2 {
		t.Fatalf("expected first file to have 2 hunks, got %d", got)
	}
	if got := len(files[1].hunks); got != 1 {
		t.Fatalf("expected second file to have 1 hunk, got %d", got)
	}
	if !strings.Contains(files[0].header, "a/a.txt") {
		t.Fatalf("expected first header to mention a.txt, got %q", files[0].header)
	}
}

func TestSampleDiffPatchRepresentsMultipleFiles(t *testing.T) {
	diff := buildMultiFileDiff([]string{"alpha.txt", "beta.txt", "gamma.txt"}, 20)
	files := parseDiffFiles(diff)

	sampled, represented, truncated := sampleDiffPatch(files, diff, 900)

	if !truncated {
		t.Fatal("expected diff sampling to truncate oversized diff")
	}
	if represented < 2 {
		t.Fatalf("expected at least 2 represented files, got %d", represented)
	}
	for _, file := range []string{"alpha.txt", "beta.txt"} {
		if !strings.Contains(sampled, file) {
			t.Fatalf("expected sampled diff to include %s, got %q", file, sampled)
		}
	}
	if !strings.Contains(sampled, diffSamplingNotice[:len(diffSamplingNotice)-1]) {
		t.Fatalf("expected sampled diff to contain sampling notice, got %q", sampled)
	}
}

func TestSampleDiffPatchPrefersFirstHunkOfEachFile(t *testing.T) {
	diff := strings.Join([]string{
		"diff --git a/alpha.txt b/alpha.txt",
		"index 111..222 100644",
		"--- a/alpha.txt",
		"+++ b/alpha.txt",
		"@@ -1 +1 @@",
		"-alpha-old-1",
		"+alpha-new-1",
		repeatPatchLine("+alpha-context-1", 40),
		"@@ -50 +50 @@",
		"-alpha-old-2",
		"+alpha-new-2",
		repeatPatchLine("+alpha-context-2", 40),
		"diff --git a/beta.txt b/beta.txt",
		"index 333..444 100644",
		"--- a/beta.txt",
		"+++ b/beta.txt",
		"@@ -1 +1 @@",
		"-beta-old-1",
		"+beta-new-1",
		repeatPatchLine("+beta-context-1", 40),
		"",
	}, "\n")

	files := parseDiffFiles(diff)
	sampled, represented, truncated := sampleDiffPatch(files, diff, 1200)

	if !truncated {
		t.Fatal("expected truncation for constrained budget")
	}
	if represented != 2 {
		t.Fatalf("expected both files to be represented, got %d", represented)
	}
	if !strings.Contains(sampled, "+alpha-new-1") {
		t.Fatalf("expected first hunk of alpha.txt to be kept, got %q", sampled)
	}
	if !strings.Contains(sampled, "+beta-new-1") {
		t.Fatalf("expected first hunk of beta.txt to be kept, got %q", sampled)
	}
	if strings.Contains(sampled, "+alpha-new-2") {
		t.Fatalf("expected second hunk of alpha.txt to be dropped before beta first hunk, got %q", sampled)
	}
}

func TestPrepareDiffForPromptKeepsFullDiffWhenUnderBudget(t *testing.T) {
	diffStat := " alpha.txt | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n"
	diffPatch := strings.Join([]string{
		"diff --git a/alpha.txt b/alpha.txt",
		"index 111..222 100644",
		"--- a/alpha.txt",
		"+++ b/alpha.txt",
		"@@ -1 +1 @@",
		"-old",
		"+new",
		"",
	}, "\n")

	trimmedStat, sampledPatch, result := prepareDiffForPrompt(diffStat, diffPatch, 6000)

	if result.sampled {
		t.Fatal("expected full diff to fit under budget")
	}
	if strings.TrimSpace(trimmedStat) != strings.TrimSpace(diffStat) {
		t.Fatalf("expected full diff stat, got %q", trimmedStat)
	}
	if strings.TrimSpace(sampledPatch) != strings.TrimSpace(diffPatch) {
		t.Fatalf("expected full diff patch, got %q", sampledPatch)
	}
	if result.representedFiles != 1 || result.totalFiles != 1 {
		t.Fatalf("unexpected file counts: %+v", result)
	}
}

func buildMultiFileDiff(files []string, repeat int) string {
	var sections []string
	for _, file := range files {
		sections = append(sections,
			"diff --git a/"+file+" b/"+file,
			"index 111..222 100644",
			"--- a/"+file,
			"+++ b/"+file,
			"@@ -1 +1 @@",
			"-old-"+file,
			"+new-"+file,
			repeatPatchLine("+context-"+file, repeat),
		)
	}
	sections = append(sections, "")
	return strings.Join(sections, "\n")
}

func repeatPatchLine(prefix string, repeat int) string {
	var lines []string
	for i := 0; i < repeat; i++ {
		lines = append(lines, prefix)
	}
	return strings.Join(lines, "\n")
}
