package main

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"unicode/utf8"
)

const maxSubjectRunes = 72

func sanitizeMessage(message string) string {
	message = strings.ReplaceAll(message, "\r\n", "\n")
	message = strings.TrimSpace(message)
	message = strings.TrimPrefix(message, "```")
	message = strings.TrimSuffix(message, "```")
	message = strings.TrimSpace(message)

	lines := strings.Split(message, "\n")
	cleaned := make([]string, 0, len(lines))
	for _, line := range lines {
		if strings.HasPrefix(strings.TrimSpace(line), "```") {
			continue
		}
		cleaned = append(cleaned, strings.TrimRight(line, " \t"))
	}

	return strings.TrimSpace(strings.Join(cleaned, "\n"))
}

func validateMessage(message string) error {
	if message == "" {
		return errors.New("chat completion returned an empty message")
	}

	subject := firstLine(message)
	if utf8.RuneCountInString(subject) > maxSubjectRunes {
		return fmt.Errorf("generated subject exceeds %d characters", maxSubjectRunes)
	}

	return nil
}

func writeMessageFile(path, message string) error {
	existing, err := os.ReadFile(path)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}

	content := message + "\n"
	if trimmed := strings.TrimSpace(string(existing)); trimmed != "" {
		content += "\n" + strings.TrimLeft(string(existing), "\n")
	}

	return os.WriteFile(path, []byte(content), 0o644)
}

func trimToUTF8Bytes(input string, maxBytes int) string {
	if maxBytes <= 0 {
		return ""
	}
	if len(input) <= maxBytes {
		return input
	}

	trimmed := input[:maxBytes]
	for !utf8.ValidString(trimmed) && len(trimmed) > 0 {
		trimmed = trimmed[:len(trimmed)-1]
	}
	return trimmed
}

func trimWithNoticeAtLineBoundary(input string, maxBytes int, notice string) (string, bool) {
	if maxBytes <= 0 {
		return "", len(input) > 0
	}
	if len(input) <= maxBytes {
		return input, false
	}

	available := maxBytes - len(notice)
	if available <= 0 {
		return trimToUTF8Bytes(notice, maxBytes), true
	}

	trimmed := trimToUTF8Bytes(input, available)
	if idx := strings.LastIndex(trimmed, "\n"); idx > 0 {
		trimmed = trimmed[:idx+1]
	}
	if trimmed == "" {
		trimmed = trimToUTF8Bytes(input, available)
	}
	if trimmed == "" {
		return trimToUTF8Bytes(notice, maxBytes), true
	}

	return trimmed + notice, true
}

func firstLine(input string) string {
	if idx := strings.IndexByte(input, '\n'); idx >= 0 {
		return input[:idx]
	}
	return input
}
