use std::time::Duration;

const DEFAULT_CHUNK_INTERVAL: Duration = Duration::from_millis(400);
const MIN_SCROLL_SPEED: f64 = 4.0;
const MAX_SCROLL_SPEED: f64 = 48.0;
const MAX_SCROLL_ACCELERATION: f64 = 96.0;
const SPEED_SMOOTHING: f64 = 0.35;

#[derive(Debug, Default)]
pub(super) struct ScrollController {
    position: f64,
    target: f64,
    velocity: f64,
    desired_velocity: f64,
    last_chunk_at: Option<Duration>,
}

impl ScrollController {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn set_target(&mut self, target: usize, now: Duration) {
        let target = target as f64;
        let distance = (target - self.target).max(0.0);

        if distance > f64::EPSILON {
            let interval = self
                .last_chunk_at
                .map(|last| now.saturating_sub(last))
                .unwrap_or(DEFAULT_CHUNK_INTERVAL);
            let seconds = interval.as_secs_f64().max(0.001);
            let sampled_velocity = (distance / seconds).clamp(MIN_SCROLL_SPEED, MAX_SCROLL_SPEED);
            self.desired_velocity = if self.last_chunk_at.is_some() {
                lerp(self.desired_velocity, sampled_velocity, SPEED_SMOOTHING)
            } else {
                sampled_velocity
            };
            self.target = target;
        }

        self.last_chunk_at = Some(now);
    }

    pub(super) fn advance(&mut self, elapsed: Duration) {
        if self.at_target() {
            self.velocity = 0.0;
            return;
        }

        let seconds = elapsed.as_secs_f64();
        if seconds <= 0.0 {
            return;
        }

        let max_velocity_change = MAX_SCROLL_ACCELERATION * seconds;
        self.velocity = approach(self.velocity, self.desired_velocity, max_velocity_change);
        self.position = (self.position + self.velocity.max(0.0) * seconds).min(self.target);

        if self.at_target() {
            self.position = self.target;
            self.velocity = 0.0;
            self.desired_velocity = 0.0;
        }
    }

    pub(super) fn position(&self) -> usize {
        self.position.floor() as usize
    }

    pub(super) fn at_target(&self) -> bool {
        self.position + f64::EPSILON >= self.target
    }

    #[cfg(test)]
    fn velocity(&self) -> f64 {
        self.velocity
    }

    pub(super) fn target(&self) -> usize {
        self.target as usize
    }
}

fn approach(current: f64, target: f64, maximum_change: f64) -> f64 {
    if current < target {
        (current + maximum_change).min(target)
    } else {
        (current - maximum_change).max(target)
    }
}

fn lerp(current: f64, target: f64, amount: f64) -> f64 {
    current + (target - current) * amount
}

#[cfg(test)]
mod tests {
    use super::{MAX_SCROLL_ACCELERATION, MAX_SCROLL_SPEED, ScrollController};
    use std::time::Duration;

    #[test]
    fn chunk_targets_only_move_forward() {
        let mut scroll = ScrollController::default();
        scroll.set_target(24, Duration::ZERO);
        scroll.set_target(40, Duration::from_millis(100));
        scroll.set_target(12, Duration::from_millis(200));

        assert_eq!(scroll.target(), 40);
    }

    #[test]
    fn elapsed_time_controls_progress_not_render_count() {
        let mut with_bursty_updates = ScrollController::default();
        with_bursty_updates.set_target(40, Duration::ZERO);
        with_bursty_updates.set_target(40, Duration::from_millis(10));
        with_bursty_updates.set_target(40, Duration::from_millis(20));
        with_bursty_updates.advance(Duration::from_millis(80));

        let mut without_updates = ScrollController::default();
        without_updates.set_target(40, Duration::ZERO);
        without_updates.advance(Duration::from_millis(80));

        assert_eq!(with_bursty_updates.position(), without_updates.position());
    }

    #[test]
    fn speed_and_acceleration_are_bounded() {
        let mut scroll = ScrollController::default();
        scroll.set_target(200, Duration::ZERO);
        scroll.advance(Duration::from_millis(80));
        let first_velocity = scroll.velocity();

        scroll.set_target(300, Duration::from_millis(80));
        scroll.advance(Duration::from_millis(80));
        let second_velocity = scroll.velocity();

        assert!(first_velocity <= MAX_SCROLL_SPEED);
        assert!(second_velocity <= MAX_SCROLL_SPEED);
        assert!(second_velocity - first_velocity <= MAX_SCROLL_ACCELERATION * 0.08);
    }

    #[test]
    fn movement_does_not_overshoot_target() {
        let mut scroll = ScrollController::default();
        scroll.set_target(20, Duration::ZERO);
        scroll.advance(Duration::from_secs(10));

        assert!(scroll.at_target());
        assert_eq!(scroll.position(), 20);
    }
}
