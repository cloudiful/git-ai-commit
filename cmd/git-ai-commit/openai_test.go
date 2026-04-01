package main

import "testing"

func TestParseJSONChatCompletionResponse(t *testing.T) {
	body := []byte(`{"choices":[{"message":{"content":"feat: add parser"}}]}`)

	message, err := parseChatCompletionResponse(200, "application/json", body)
	if err != nil {
		t.Fatalf("expected JSON response to parse, got error: %v", err)
	}
	if message != "feat: add parser" {
		t.Fatalf("unexpected message: %q", message)
	}
}

func TestParseStreamingChatCompletionResponse(t *testing.T) {
	body := []byte("" +
		"data: {\"choices\":[{\"delta\":{\"content\":\"feat:\"}}]}\n\n" +
		"data: {\"choices\":[{\"delta\":{\"content\":\" add parser\"}}]}\n\n" +
		"data: [DONE]\n")

	message, err := parseChatCompletionResponse(200, "text/event-stream", body)
	if err != nil {
		t.Fatalf("expected SSE response to parse, got error: %v", err)
	}
	if message != "feat: add parser" {
		t.Fatalf("unexpected message: %q", message)
	}
}

func TestParseStreamingChatCompletionResponseRejectsInvalidChunk(t *testing.T) {
	body := []byte("data: definitely-not-json\n")

	_, err := parseChatCompletionResponse(200, "text/event-stream", body)
	if err == nil {
		t.Fatal("expected invalid SSE chunk to fail")
	}
}
