use futures::StreamExt;
use reqwest::Response;
use reqwest::header::CONTENT_TYPE;

const TAIL_PREVIEW_LIMIT: usize = 512;

#[derive(Clone, Debug, Default)]
pub(super) struct SseStats {
    pub(super) chunk_count: usize,
    pub(super) total_bytes: usize,
    pub(super) event_count: usize,
    pub(super) content_type: Option<String>,
    pub(super) last_payload_preview: Option<String>,
}

pub(super) async fn collect_sse_events(
    response: Response,
    mut on_payload: impl FnMut(&str) -> Result<bool, String>,
) -> Result<SseStats, String> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut byte_stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut stats = SseStats {
        content_type,
        ..SseStats::default()
    };

    while let Some(chunk) = byte_stream.next().await {
        let chunk = chunk.map_err(|err| {
            format_stream_error(
                &format!("stream failed: {err}"),
                &stats,
                preview_bytes(&buffer),
            )
        })?;
        stats.chunk_count += 1;
        stats.total_bytes += chunk.len();
        buffer.extend_from_slice(&chunk);

        while let Some(payload) = take_next_sse_payload(&mut buffer)
            .map_err(|err| format_stream_error(&err, &stats, preview_bytes(&buffer)))?
        {
            stats.event_count += 1;
            stats.last_payload_preview = Some(preview_text(&payload));
            if !on_payload(&payload)
                .map_err(|err| format_stream_error(&err, &stats, preview_bytes(&buffer)))?
            {
                return Ok(stats);
            }
        }
    }

    while let Some(payload) = take_next_sse_payload_with_eof(&mut buffer)
        .map_err(|err| format_stream_error(&err, &stats, preview_bytes(&buffer)))?
    {
        stats.event_count += 1;
        stats.last_payload_preview = Some(preview_text(&payload));
        if !on_payload(&payload)
            .map_err(|err| format_stream_error(&err, &stats, preview_bytes(&buffer)))?
        {
            return Ok(stats);
        }
    }

    Ok(stats)
}

fn format_stream_error(message: &str, stats: &SseStats, tail_preview: Option<String>) -> String {
    let mut parts = vec![format!(
        "after {} chunks, {} bytes, {} SSE events",
        stats.chunk_count, stats.total_bytes, stats.event_count
    )];

    if let Some(content_type) = stats.content_type.as_deref() {
        parts.push(format!("content-type: {content_type}"));
    }

    if let Some(payload) = stats.last_payload_preview.as_deref() {
        parts.push(format!("last SSE payload preview: {payload}"));
    }

    if let Some(tail) = tail_preview {
        parts.push(format!("response tail preview: {tail}"));
    }

    format!("{message} ({})", parts.join("; "))
}

fn take_next_sse_payload(buffer: &mut Vec<u8>) -> Result<Option<String>, String> {
    take_next_sse_payload_inner(buffer, false)
}

fn take_next_sse_payload_with_eof(buffer: &mut Vec<u8>) -> Result<Option<String>, String> {
    take_next_sse_payload_inner(buffer, true)
}

fn take_next_sse_payload_inner(
    buffer: &mut Vec<u8>,
    flush_eof: bool,
) -> Result<Option<String>, String> {
    let Some((event_len, separator_len)) = find_sse_event_boundary(buffer).or_else(|| {
        if flush_eof && !buffer.is_empty() {
            Some((buffer.len(), 0))
        } else {
            None
        }
    }) else {
        return Ok(None);
    };

    let event_bytes = buffer[..event_len].to_vec();
    buffer.drain(..event_len + separator_len);
    let event = std::str::from_utf8(&event_bytes)
        .map_err(|err| format!("stream failed: invalid utf8 SSE payload: {err}"))?;

    let mut data_lines = Vec::new();
    let mut saw_non_sse_line = false;
    for line in event.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(payload) = line.strip_prefix("data:") {
            data_lines.push(payload.trim_start().to_string());
            continue;
        }
        if line.is_empty()
            || line.starts_with(':')
            || line.starts_with("event:")
            || line.starts_with("id:")
            || line.starts_with("retry:")
        {
            continue;
        }
        saw_non_sse_line = true;
    }

    if data_lines.is_empty() {
        if saw_non_sse_line {
            return Err(
                "stream failed: provider returned non-SSE response body while stream=true"
                    .to_string(),
            );
        }
        return Ok(None);
    }

    Ok(Some(data_lines.join("\n")))
}

fn find_sse_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut idx = 0usize;
    while idx < buffer.len() {
        if idx + 3 < buffer.len() && &buffer[idx..idx + 4] == b"\r\n\r\n" {
            return Some((idx, 4));
        }
        if idx + 1 < buffer.len() && &buffer[idx..idx + 2] == b"\n\n" {
            return Some((idx, 2));
        }
        idx += 1;
    }
    None
}

fn preview_bytes(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    let start = bytes.len().saturating_sub(TAIL_PREVIEW_LIMIT);
    Some(preview_text(&String::from_utf8_lossy(&bytes[start..])))
}

fn preview_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let preview = chars.by_ref().take(240).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::{SseStats, format_stream_error, take_next_sse_payload_with_eof};

    #[test]
    fn includes_progress_and_tail_preview_in_error_message() {
        let stats = SseStats {
            chunk_count: 3,
            total_bytes: 128,
            event_count: 7,
            content_type: Some("application/json".to_string()),
            last_payload_preview: Some("{\"type\":\"response.reasoning_text.delta\"}".to_string()),
        };

        let message = format_stream_error(
            "stream failed: error decoding response body",
            &stats,
            Some("{\"dangling\":true}".to_string()),
        );

        assert!(message.contains("after 3 chunks, 128 bytes, 7 SSE events"));
        assert!(message.contains("content-type: application/json"));
        assert!(message.contains("last SSE payload preview"));
        assert!(message.contains("response tail preview"));
    }

    #[test]
    fn rejects_non_sse_body_at_eof() {
        let mut buffer = br#"{"id":"resp_123","output_text":"feat: add parser"}"#.to_vec();
        let err = take_next_sse_payload_with_eof(&mut buffer).unwrap_err();

        assert_eq!(
            err,
            "stream failed: provider returned non-SSE response body while stream=true"
        );
    }
}
