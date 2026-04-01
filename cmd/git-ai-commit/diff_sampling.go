package main

import (
	"strings"
)

const (
	diffPromptReserve         = 1024
	diffStatSafetyCap         = 8192
	diffSamplingNotice        = "[diff selectively sampled to fit budget]\n"
	diffStatTruncatedNotice   = "[diff stat truncated]\n"
	diffHeaderTruncatedNotice = "[file header truncated]\n"
	diffHunkTruncatedNotice   = "[hunk truncated]\n"
)

type diffSamplingResult struct {
	totalFiles       int
	representedFiles int
	sampled          bool
	statTruncated    bool
}

func prepareDiffForPrompt(diffStat, diffPatch string, maxBytes int) (string, string, diffSamplingResult) {
	files := parseDiffFiles(diffPatch)
	result := diffSamplingResult{
		totalFiles:       len(files),
		representedFiles: len(files),
	}

	statCap := diffStatSafetyCap
	if maxBytes > 0 && statCap > maxBytes/3 {
		statCap = maxBytes / 3
	}
	if statCap < 512 {
		statCap = 512
	}

	trimmedStat, statTruncated := trimWithNoticeAtLineBoundary(diffStat, statCap, diffStatTruncatedNotice)
	result.statTruncated = statTruncated

	patchBudget := maxBytes - diffPromptReserve - len(trimmedStat)
	if patchBudget < len(diffSamplingNotice) {
		patchBudget = len(diffSamplingNotice)
	}

	sampledPatch, representedFiles, sampled := sampleDiffPatch(files, strings.ReplaceAll(diffPatch, "\r\n", "\n"), patchBudget)
	result.representedFiles = representedFiles
	result.sampled = sampled

	return trimmedStat, sampledPatch, result
}

func sampleDiffPatch(files []diffFile, rawDiff string, budget int) (string, int, bool) {
	if strings.TrimSpace(rawDiff) == "" || budget <= 0 {
		return "", 0, false
	}
	if len(rawDiff) <= budget {
		return strings.TrimSpace(rawDiff), len(files), false
	}
	if len(files) == 0 {
		trimmed, _ := trimWithNoticeAtLineBoundary(rawDiff, budget, diffSamplingNotice)
		return strings.TrimSpace(trimmed), 1, true
	}

	var builder strings.Builder
	remaining := budget
	represented := 0
	headerAdded := make([]bool, len(files))
	firstHunkHandled := make([]bool, len(files))

	if appendSample(&builder, &remaining, diffSamplingNotice) == 0 {
		return strings.TrimSpace(trimToUTF8Bytes(diffSamplingNotice, budget)), 0, true
	}

	for i, file := range files {
		quota := phaseQuota(remaining, len(files)-i, 96, 320)
		headerSample, _ := trimWithNoticeAtLineBoundary(file.header, quota, diffHeaderTruncatedNotice)
		if appendSample(&builder, &remaining, headerSample) > 0 {
			headerAdded[i] = true
			represented++
		}
	}

	for i, file := range files {
		if len(file.hunks) == 0 || remaining <= 0 {
			continue
		}
		quota := phaseQuota(remaining, countFilesWithPendingFirstHunk(files, firstHunkHandled, i), 192, 960)
		hunkSample, full := trimWithNoticeAtLineBoundary(file.hunks[0], quota, diffHunkTruncatedNotice)
		if appendSample(&builder, &remaining, hunkSample) > 0 {
			firstHunkHandled[i] = true
			if !headerAdded[i] {
				headerAdded[i] = true
				represented++
			}
		}
		if !full {
			firstHunkHandled[i] = true
		}
	}

	for i, file := range files {
		start := 0
		if len(file.hunks) > 0 && firstHunkHandled[i] {
			start = 1
		}
		for h := start; h < len(file.hunks) && remaining > 0; h++ {
			if len(file.hunks[h]) <= remaining {
				appendSample(&builder, &remaining, file.hunks[h])
				if !headerAdded[i] {
					headerAdded[i] = true
					represented++
				}
				continue
			}
			hunkSample, _ := trimWithNoticeAtLineBoundary(file.hunks[h], remaining, diffHunkTruncatedNotice)
			if appendSample(&builder, &remaining, hunkSample) > 0 && !headerAdded[i] {
				headerAdded[i] = true
				represented++
			}
			remaining = 0
		}
	}

	return strings.TrimSpace(builder.String()), represented, true
}

func countFilesWithPendingFirstHunk(files []diffFile, firstHunkHandled []bool, start int) int {
	count := 0
	for i := start; i < len(files); i++ {
		if len(files[i].hunks) > 0 && !firstHunkHandled[i] {
			count++
		}
	}
	if count == 0 {
		return 1
	}
	return count
}

func phaseQuota(remaining, slots, minQuota, maxQuota int) int {
	if remaining <= 0 {
		return 0
	}
	if slots <= 0 {
		return remaining
	}
	quota := remaining / slots
	if quota < minQuota {
		quota = minQuota
	}
	if quota > maxQuota {
		quota = maxQuota
	}
	if quota > remaining {
		quota = remaining
	}
	return quota
}

func appendSample(builder *strings.Builder, remaining *int, chunk string) int {
	if *remaining <= 0 || chunk == "" {
		return 0
	}
	if len(chunk) > *remaining {
		chunk = trimToUTF8Bytes(chunk, *remaining)
	}
	if chunk == "" {
		return 0
	}
	builder.WriteString(chunk)
	*remaining -= len(chunk)
	return len(chunk)
}
