package gitai

import "strings"

type diffFile struct {
	header string
	hunks  []string
}

func parseDiffFiles(rawDiff string) []diffFile {
	if strings.TrimSpace(rawDiff) == "" {
		return nil
	}

	lines := strings.SplitAfter(strings.ReplaceAll(rawDiff, "\r\n", "\n"), "\n")
	files := make([]diffFile, 0)
	var current *diffFile
	var currentHunk strings.Builder
	inHunk := false

	flushHunk := func() {
		if current == nil || currentHunk.Len() == 0 {
			return
		}
		current.hunks = append(current.hunks, currentHunk.String())
		currentHunk.Reset()
	}

	flushFile := func() {
		if current == nil {
			return
		}
		flushHunk()
		files = append(files, *current)
		current = nil
		inHunk = false
	}

	for _, line := range lines {
		if strings.HasPrefix(line, "diff --git ") {
			flushFile()
			current = &diffFile{header: line}
			continue
		}
		if current == nil {
			current = &diffFile{}
		}
		if strings.HasPrefix(line, "@@") {
			flushHunk()
			currentHunk.WriteString(line)
			inHunk = true
			continue
		}
		if inHunk {
			currentHunk.WriteString(line)
			continue
		}
		current.header += line
	}

	flushFile()
	if len(files) == 0 && current != nil {
		files = append(files, *current)
	}
	return files
}
