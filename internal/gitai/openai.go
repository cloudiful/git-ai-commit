package gitai

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

type chatCompletionRequest struct {
	Model       string        `json:"model"`
	Messages    []chatMessage `json:"messages"`
	Temperature float64       `json:"temperature,omitempty"`
	MaxTokens   int           `json:"max_tokens,omitempty"`
	Stream      bool          `json:"stream"`
}

type chatMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type chatCompletionResponse struct {
	Choices []struct {
		Message struct {
			Content string `json:"content"`
		} `json:"message"`
	} `json:"choices"`
	Error *struct {
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

type chatCompletionChunk struct {
	Choices []struct {
		Delta struct {
			Content string `json:"content"`
		} `json:"delta"`
	} `json:"choices"`
	Error *struct {
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

func generateMessage(ctx context.Context, cfg config, repoCtx repoContext) (string, generationMetrics, error) {
	requestBody := chatCompletionRequest{
		Model: cfg.model,
		Messages: []chatMessage{
			{
				Role:    "system",
				Content: "You write Git commit messages. Output only the commit message text, no code fences, no commentary. Use English Conventional Commit style. Keep the first line within 72 characters. Include a short body only when the change is complex enough to benefit from it. Do not invent behavior not present in the diff.",
			},
			{
				Role:    "user",
				Content: buildPrompt(repoCtx),
			},
		},
		Temperature: 0.2,
		MaxTokens:   220,
		Stream:      false,
	}

	body, err := json.Marshal(requestBody)
	if err != nil {
		return "", generationMetrics{}, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, chatCompletionsURL(cfg.apiBase), bytes.NewReader(body))
	if err != nil {
		return "", generationMetrics{}, err
	}
	req.Header.Set("Authorization", "Bearer "+cfg.apiKey)
	req.Header.Set("Content-Type", "application/json")

	apiStartedAt := time.Now()
	resp, err := newHTTPClient(cfg).Do(req)
	if err != nil {
		return "", generationMetrics{}, err
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return "", generationMetrics{}, err
	}
	metrics := generationMetrics{apiDuration: time.Since(apiStartedAt)}

	message, err := parseChatCompletionResponse(resp.StatusCode, resp.Header.Get("Content-Type"), respBody)
	if err != nil {
		return "", metrics, err
	}
	if err := validateMessage(message); err != nil {
		return "", metrics, err
	}

	return message, metrics, nil
}

func newHTTPClient(cfg config) *http.Client {
	transport := http.DefaultTransport.(*http.Transport).Clone()
	if cfg.useEnvProxy {
		transport.Proxy = http.ProxyFromEnvironment
	} else {
		transport.Proxy = func(*http.Request) (*url.URL, error) {
			return nil, nil
		}
	}

	return &http.Client{
		Transport: transport,
	}
}

func parseChatCompletionResponse(statusCode int, contentType string, body []byte) (string, error) {
	trimmedBody := strings.TrimSpace(string(body))
	if strings.Contains(strings.ToLower(contentType), "text/event-stream") || strings.HasPrefix(trimmedBody, "data:") {
		return parseStreamingChatCompletion(statusCode, trimmedBody)
	}
	return parseJSONChatCompletion(statusCode, body)
}

func parseJSONChatCompletion(statusCode int, body []byte) (string, error) {
	var parsed chatCompletionResponse
	if err := json.Unmarshal(body, &parsed); err != nil {
		if statusCode >= 400 {
			return "", fmt.Errorf("chat completion failed: %s", strings.TrimSpace(string(body)))
		}
		return "", err
	}

	if statusCode >= 400 {
		if parsed.Error != nil && strings.TrimSpace(parsed.Error.Message) != "" {
			return "", errors.New(parsed.Error.Message)
		}
		return "", fmt.Errorf("chat completion failed with status %d", statusCode)
	}

	if len(parsed.Choices) == 0 {
		return "", errors.New("chat completion returned no choices")
	}

	return sanitizeMessage(parsed.Choices[0].Message.Content), nil
}

func parseStreamingChatCompletion(statusCode int, body string) (string, error) {
	lines := strings.Split(strings.ReplaceAll(body, "\r\n", "\n"), "\n")
	var content strings.Builder
	var providerErr string
	sawChunk := false

	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" || !strings.HasPrefix(line, "data:") {
			continue
		}

		payload := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
		if payload == "" {
			continue
		}
		if payload == "[DONE]" {
			break
		}

		sawChunk = true
		var chunk chatCompletionChunk
		if err := json.Unmarshal([]byte(payload), &chunk); err != nil {
			return "", fmt.Errorf("invalid streaming chat completion chunk: %w", err)
		}
		if chunk.Error != nil && strings.TrimSpace(chunk.Error.Message) != "" {
			providerErr = chunk.Error.Message
			continue
		}
		for _, choice := range chunk.Choices {
			content.WriteString(choice.Delta.Content)
		}
	}

	if statusCode >= 400 {
		if providerErr != "" {
			return "", errors.New(providerErr)
		}
		return "", fmt.Errorf("chat completion failed with status %d", statusCode)
	}
	if !sawChunk {
		return "", errors.New("chat completion returned no stream chunks")
	}

	return sanitizeMessage(content.String()), nil
}

func buildPrompt(repoCtx repoContext) string {
	var b strings.Builder
	b.WriteString("Generate a commit message from the staged changes.\n\n")
	b.WriteString("Repository: ")
	b.WriteString(repoCtx.repoName)
	b.WriteString("\nBranch: ")
	b.WriteString(repoCtx.branchName)
	b.WriteString("\nChanged files: ")
	b.WriteString(strconv.Itoa(repoCtx.changedFileCount))
	b.WriteString("\nRepresented files in diff sample: ")
	b.WriteString(strconv.Itoa(repoCtx.representedFileCount))
	b.WriteString("/")
	b.WriteString(strconv.Itoa(repoCtx.changedFileCount))
	b.WriteString("\nDiff coverage: ")
	if repoCtx.diffTruncated {
		b.WriteString("selective sample within byte budget")
	} else {
		b.WriteString("full")
	}
	if repoCtx.diffStatTruncated {
		b.WriteString("\nDiff stat coverage: truncated")
	}
	b.WriteString("\n\nDiff stat:\n")
	if repoCtx.diffStat == "" {
		b.WriteString("(empty)\n")
	} else {
		b.WriteString(repoCtx.diffStat)
		b.WriteString("\n")
	}
	b.WriteString("\nStaged diff:\n")
	if repoCtx.diffPatch == "" {
		b.WriteString("(empty)\n")
	} else {
		b.WriteString(repoCtx.diffPatch)
		b.WriteString("\n")
	}
	return b.String()
}

func chatCompletionsURL(base string) string {
	base = strings.TrimRight(strings.TrimSpace(base), "/")

	switch {
	case strings.HasSuffix(base, "/chat/completions"):
		return base
	case strings.HasSuffix(base, "/v1"):
		return base + "/chat/completions"
	default:
		return base + "/v1/chat/completions"
	}
}
