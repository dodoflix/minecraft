//! Server side of `InterpolationHandler`: only the `InterpolationTracker` part (`NoOp` / `SteppedInterpolationTracker`)
//! exists on the server; the client-side interpolation is irrelevant here.

use mc_core::phys::Vec3;
use mc_protocol::packets::play::{PositionPath, PositionStep};

#[derive(Clone, Debug, Default)]
pub enum InterpolationTracker {
    #[default]
    NoOp,
    Stepped(SteppedInterpolationTracker),
}

#[derive(Clone, Debug)]
pub struct SteppedInterpolationTracker {
    tracked_steps: Vec<PositionStep>,
    ticks_since_last_step: i32,
    predicted_delta: Vec3,
}

impl Default for SteppedInterpolationTracker {
    fn default() -> Self {
        Self {
            tracked_steps: Vec::new(),
            ticks_since_last_step: -1,
            predicted_delta: Vec3::ZERO,
        }
    }
}

impl SteppedInterpolationTracker {
    fn add_step(&mut self, pos: Vec3) {
        if self.ticks_since_last_step > 0 {
            self.tracked_steps.push(PositionStep {
                position: pos,
                tick_offset: self.ticks_since_last_step,
            });
            self.ticks_since_last_step = 0;
        }
    }
}

impl InterpolationTracker {
    /// `InterpolationHandler.applyPredictedMovement(delta)`.
    pub fn apply_predicted_movement(&mut self, delta: Vec3) {
        if let Self::Stepped(t) = self {
            t.predicted_delta = t.predicted_delta.add(delta);
        }
    }

    /// `updateTracking(trackingPos)`; `sync_position` is `entity.syncPosition` (cleared when a step is recorded).
    pub fn update_tracking(&mut self, tracking_pos: Vec3, sync_position: &mut bool) {
        if let Self::Stepped(t) = self {
            if t.predicted_delta.length_sqr() > f64::from(1.0E-5f32) {
                let d = t.predicted_delta;
                for s in &mut t.tracked_steps {
                    s.position = s.position.add(d);
                }
            }
            t.predicted_delta = Vec3::ZERO;
            t.ticks_since_last_step += 1;
            if *sync_position {
                *sync_position = false;
                t.add_step(tracking_pos);
            }
        }
    }

    /// `getPositionPath(currentPosition)`.
    pub fn get_position_path(&mut self, current: Vec3) -> PositionPath {
        match self {
            Self::NoOp => PositionPath::Linear(current),
            Self::Stepped(t) => {
                t.add_step(current);
                if t.tracked_steps.is_empty() {
                    PositionPath::Linear(current)
                } else {
                    PositionPath::Stepped(t.tracked_steps.clone())
                }
            }
        }
    }

    pub fn clear(&mut self) {
        if let Self::Stepped(t) = self {
            t.tracked_steps.clear();
            t.ticks_since_last_step = 0;
        }
    }
}

/// `PositionPath.endPosition()`.
pub fn end_position(path: &PositionPath) -> Vec3 {
    match path {
        PositionPath::Linear(p) => *p,
        PositionPath::Stepped(steps) => steps.last().map(|s| s.position).unwrap_or(Vec3::ZERO),
    }
}
