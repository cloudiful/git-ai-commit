package gitai

import (
	"strings"
	"testing"
	"unicode/utf8"
)

func TestTrimToUTF8BytesKeepsValidUTF8(t *testing.T) {
	input := "你好abc"
	trimmed := trimToUTF8Bytes(input, 5)

	if !utf8.ValidString(trimmed) {
		t.Fatalf("expected valid utf-8, got %q", trimmed)
	}
	if trimmed != "你" {
		t.Fatalf("expected first rune only, got %q", trimmed)
	}
}

func TestTrimWithNoticeAtLineBoundaryPrefersWholeLines(t *testing.T) {
	input := "line-1\nline-2\nline-3\nline-4\nline-5\n"
	trimmed, truncated := trimWithNoticeAtLineBoundary(input, len("line-1\nline-2\n")+len(diffHunkTruncatedNotice), diffHunkTruncatedNotice)

	if !truncated {
		t.Fatal("expected truncation")
	}
	if !strings.HasPrefix(trimmed, "line-1\nline-2\n") {
		t.Fatalf("expected to keep whole lines, got %q", trimmed)
	}
	if !strings.HasSuffix(trimmed, diffHunkTruncatedNotice) {
		t.Fatalf("expected truncation notice, got %q", trimmed)
	}
}

func TestTrimWithNoticeAtLineBoundaryFallsBackToNoticeWhenBudgetTiny(t *testing.T) {
	trimmed, truncated := trimWithNoticeAtLineBoundary("abcdef", 4, diffHunkTruncatedNotice)

	if !truncated {
		t.Fatal("expected truncation")
	}
	if trimmed == "" {
		t.Fatal("expected non-empty output")
	}
	if len(trimmed) > 4 {
		t.Fatalf("expected output to respect byte budget, got %d bytes", len(trimmed))
	}
}

func TestSanitizeMessageRemovesCodeFences(t *testing.T) {
	input := "```text\nfeat: add tests\n\nbody\n```"
	sanitized := sanitizeMessage(input)

	if strings.Contains(sanitized, "```") {
		t.Fatalf("expected code fences removed, got %q", sanitized)
	}
	if !strings.Contains(sanitized, "feat: add tests") {
		t.Fatalf("expected message body retained, got %q", sanitized)
	}
}
