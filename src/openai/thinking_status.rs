mod render;

#[cfg(test)]
mod tests;

use self::render::{clear_status_line, render_status};
use super::stream_palette::StreamPalette;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const FRAME_INTERVAL: Duration = Duration::from_millis(80);

pub(crate) struct ThinkingStatus {
    palette: StreamPalette,
    output_lock: Arc<Mutex<()>>,
    worker: Option<ThinkingStatusWorker>,
}

struct ThinkingStatusWorker {
    tx: mpsc::Sender<ThinkingCommand>,
    handle: JoinHandle<()>,
}

enum ThinkingCommand {
    SetPlaceholder(String),
    AppendReasoning(String),
    Stop,
}

pub(super) struct ThinkingState {
    pub(super) text: String,
    pub(super) has_reasoning_text: bool,
}

impl ThinkingStatus {
    pub(crate) fn new(output_lock: Arc<Mutex<()>>, palette: StreamPalette) -> Self {
        Self {
            palette,
            output_lock,
            worker: None,
        }
    }

    pub(crate) fn show_placeholder(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.ensure_worker()?;
        self.send_command(ThinkingCommand::SetPlaceholder(text.to_string()))
    }

    pub(crate) fn push_text(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.ensure_worker()?;
        self.send_command(ThinkingCommand::AppendReasoning(text.to_string()))
    }

    pub(crate) fn stop(&mut self) -> std::io::Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        let _ = worker.tx.send(ThinkingCommand::Stop);
        worker
            .handle
            .join()
            .map_err(|_| std::io::Error::other("thinking status worker panicked"))?;
        clear_status_line(&self.output_lock)
    }

    pub(crate) fn reset(&mut self) {
        let _ = self.stop();
    }

    fn ensure_worker(&mut self) -> std::io::Result<()> {
        if self.worker.is_some() {
            return Ok(());
        }
        let (tx, rx) = mpsc::channel();
        let output_lock = Arc::clone(&self.output_lock);
        let palette = self.palette;
        let handle = thread::Builder::new()
            .name("git-ai-commit-thinking".to_string())
            .spawn(move || run_worker(rx, output_lock, palette))
            .map_err(std::io::Error::other)?;
        self.worker = Some(ThinkingStatusWorker { tx, handle });
        Ok(())
    }

    fn send_command(&self, command: ThinkingCommand) -> std::io::Result<()> {
        let Some(worker) = &self.worker else {
            return Ok(());
        };
        worker.tx.send(command).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "thinking status worker stopped",
            )
        })
    }
}

fn run_worker(
    rx: mpsc::Receiver<ThinkingCommand>,
    output_lock: Arc<Mutex<()>>,
    palette: StreamPalette,
) {
    let mut state = ThinkingState {
        text: String::new(),
        has_reasoning_text: false,
    };
    let mut frame_index = 0usize;
    let mut active = false;
    loop {
        match rx.recv_timeout(FRAME_INTERVAL) {
            Ok(ThinkingCommand::SetPlaceholder(text)) => {
                state.text = text;
                state.has_reasoning_text = false;
                active = render_status(&output_lock, palette, frame_index, &state.text).is_ok();
                frame_index = frame_index.wrapping_add(1);
            }
            Ok(ThinkingCommand::AppendReasoning(delta)) => {
                if !state.has_reasoning_text {
                    state.text.clear();
                    state.has_reasoning_text = true;
                }
                state.text.push_str(&delta);
                active = render_status(&output_lock, palette, frame_index, &state.text).is_ok();
                frame_index = frame_index.wrapping_add(1);
            }
            Ok(ThinkingCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if active {
                    let _ = clear_status_line(&output_lock);
                }
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if state.text.is_empty() {
                    continue;
                }
                active = render_status(&output_lock, palette, frame_index, &state.text).is_ok();
                frame_index = frame_index.wrapping_add(1);
            }
        }
    }
}
