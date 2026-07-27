mod render;
mod scroll;

#[cfg(test)]
mod tests;

use self::render::{clear_status_line, render_status, scrolling_target};
use self::scroll::ScrollController;
use super::stream_palette::StreamPalette;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const FRAME_INTERVAL: Duration = Duration::from_millis(80);
const FINAL_HOLD: Duration = Duration::from_millis(160);

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
    Complete,
    Abort,
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

    pub(crate) fn complete(&mut self) -> std::io::Result<()> {
        self.stop_with(ThinkingCommand::Complete)
    }

    pub(crate) fn abort(&mut self) -> std::io::Result<()> {
        self.stop_with(ThinkingCommand::Abort)
    }

    fn stop_with(&mut self, command: ThinkingCommand) -> std::io::Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        let _ = worker.tx.send(command);
        worker
            .handle
            .join()
            .map_err(|_| std::io::Error::other("thinking status worker panicked"))?;
        clear_status_line(&self.output_lock)
    }

    pub(crate) fn reset(&mut self) {
        let _ = self.abort();
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
    let mut scroll = ScrollController::default();
    let mut frame_index = 0usize;
    let mut active = false;
    let worker_started_at = Instant::now();
    let mut last_animation_at = worker_started_at;
    let mut next_frame_at = worker_started_at + FRAME_INTERVAL;

    loop {
        let now = Instant::now();
        if now >= next_frame_at {
            let elapsed = now.saturating_duration_since(last_animation_at);
            scroll.advance(elapsed);
            last_animation_at = now;
            if !state.text.is_empty() {
                active = render_status(
                    &output_lock,
                    palette,
                    frame_index,
                    scroll.position(),
                    &state.text,
                )
                .is_ok();
            }
            frame_index = frame_index.wrapping_add(1);
            next_frame_at = now + FRAME_INTERVAL;
            continue;
        }

        match rx.recv_timeout(next_frame_at.saturating_duration_since(now)) {
            Ok(ThinkingCommand::SetPlaceholder(text)) => {
                state.text = text;
                state.has_reasoning_text = false;
                scroll.reset();
                last_animation_at = Instant::now();
                active = render_status(
                    &output_lock,
                    palette,
                    frame_index,
                    scroll.position(),
                    &state.text,
                )
                .is_ok();
            }
            Ok(ThinkingCommand::AppendReasoning(delta)) => {
                if !state.has_reasoning_text {
                    state.text.clear();
                    state.has_reasoning_text = true;
                    scroll.reset();
                }
                state.text.push_str(&delta);
                let now = Instant::now();
                let target = scrolling_target(&state.text);
                let previous_target = scroll.target();
                scroll.set_target(target, now.duration_since(worker_started_at));
                if previous_target == 0 && target > 0 {
                    last_animation_at = now;
                }
                active = render_status(
                    &output_lock,
                    palette,
                    frame_index,
                    scroll.position(),
                    &state.text,
                )
                .is_ok();
            }
            Ok(ThinkingCommand::Complete) => {
                finish_worker(
                    &state,
                    &mut scroll,
                    &output_lock,
                    palette,
                    &mut frame_index,
                    &mut last_animation_at,
                    &mut active,
                );
                return;
            }
            Ok(ThinkingCommand::Abort) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if active {
                    let _ = clear_status_line(&output_lock);
                }
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn finish_worker(
    state: &ThinkingState,
    scroll: &mut ScrollController,
    output_lock: &Arc<Mutex<()>>,
    palette: StreamPalette,
    frame_index: &mut usize,
    last_animation_at: &mut Instant,
    active: &mut bool,
) {
    if !state.has_reasoning_text {
        if *active {
            let _ = clear_status_line(output_lock);
        }
        return;
    }

    while !scroll.at_target() {
        thread::sleep(FRAME_INTERVAL);
        let now = Instant::now();
        scroll.advance(now.saturating_duration_since(*last_animation_at));
        *last_animation_at = now;
        *active = render_status(
            output_lock,
            palette,
            *frame_index,
            scroll.position(),
            &state.text,
        )
        .is_ok();
        *frame_index = frame_index.wrapping_add(1);
        if !*active {
            return;
        }
    }

    *active = render_status(
        output_lock,
        palette,
        *frame_index,
        scroll.position(),
        &state.text,
    )
    .is_ok();
    if *active {
        thread::sleep(FINAL_HOLD);
        let _ = clear_status_line(output_lock);
    }
}
