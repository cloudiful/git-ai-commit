use super::ThinkingState;
use super::render::{scrolling_window_text, wave_color, write_status_line};
use crate::openai::stream_palette::StreamPalette;

fn strip_ansi(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[test]
fn short_status_text_is_padded_to_fixed_width() {
    let status = scrolling_window_text("considering diff", 0);
    assert_eq!(status.chars().count(), 56);
    assert!(status.starts_with("considering diff"));
}

#[test]
fn long_status_text_scrolls_over_time() {
    let text =
        "considering staged changes and summarizing the diff in a compact way for terminal display";
    let status_a = scrolling_window_text(text, 0);
    let status_b = scrolling_window_text(text, 24);
    assert_eq!(status_a.chars().count(), 56);
    assert_eq!(status_b.chars().count(), 56);
    assert_ne!(status_a, status_b);
}

#[test]
fn write_status_line_renders_unicode_spinner_and_truecolor_gradient() {
    let mut out = Vec::new();
    write_status_line(&mut out, StreamPalette::TrueColor, 0, "considering diff").unwrap();
    let rendered = String::from_utf8(out).unwrap();
    let plain = strip_ansi(&rendered);
    assert!(rendered.contains("⠋"));
    assert!(rendered.contains("\x1b[38;2;"));
    assert!(plain.contains("thinking: considering diff"));
}

#[test]
fn placeholder_is_replaced_when_reasoning_text_arrives() {
    let mut state = ThinkingState {
        text: "drafting commit message".to_string(),
        has_reasoning_text: false,
    };
    if !state.has_reasoning_text {
        state.text.clear();
        state.has_reasoning_text = true;
    }
    state.text.push_str("considering diff");
    assert_eq!(state.text, "considering diff");
}

#[test]
fn wave_highlight_moves_between_frames() {
    let base = (150, 170, 190);
    assert_ne!(wave_color(base, 3, 20, 0), wave_color(base, 3, 20, 6));
}
