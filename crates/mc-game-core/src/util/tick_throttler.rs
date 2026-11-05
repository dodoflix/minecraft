//! `net.minecraft.util.TickThrottler`: rate-spam detection (chat, commands, creative drops).

pub struct TickThrottler {
    increment_step: i32,
    threshold: i32,
    count: i32,
}

impl TickThrottler {
    pub fn new(increment_step: i32, threshold: i32) -> Self {
        Self { increment_step, threshold, count: 0 }
    }

    pub fn increment(&mut self) {
        self.count = self.count.wrapping_add(self.increment_step);
    }

    pub fn tick(&mut self) {
        if self.count > 0 {
            self.count -= 1;
        }
    }

    pub fn is_under_threshold(&self) -> bool {
        self.threshold <= 0 || self.count < self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chat spam: 20 per message, decays 1/tick; threshold 20 * 10 s default `chatSpamThresholdSeconds`... checked by hand
    /// against the Java source: the 10th message within the window trips it.
    #[test]
    fn trips_after_threshold_messages_and_decays() {
        let mut t = TickThrottler::new(20, 200);
        for _ in 0..9 {
            t.increment();
            assert!(t.is_under_threshold());
        }
        t.increment();
        assert!(!t.is_under_threshold());
        for _ in 0..21 {
            t.tick();
        }
        assert!(t.is_under_threshold());
    }

    #[test]
    fn non_positive_threshold_never_trips() {
        let mut t = TickThrottler::new(20, 0);
        for _ in 0..1000 {
            t.increment();
        }
        assert!(t.is_under_threshold());
    }
}
