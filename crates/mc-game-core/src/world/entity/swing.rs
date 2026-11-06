//! `LivingEntity.SwingState`: gates `swing` (a swing started in the first half of a running one is ignored) and drives
//! the swing animation progress.
use crate::world::item::InteractionHand;
use mc_core::math::mth;
use mc_protocol::packets::play::SwingAnimation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SwingDescription {
    hand: InteractionHand,
    animation: SwingAnimation,
    duration_ticks: i32,
}

#[derive(Clone, Debug, Default)]
pub struct SwingState {
    current_swing: Option<SwingDescription>,
    ticks: i32,
    old_animation: f32,
    animation: f32,
}

impl SwingState {
    /// `startIfAble(hand, animation, durationTicks)`.
    pub fn start_if_able(
        &mut self,
        hand: InteractionHand,
        animation: SwingAnimation,
        duration_ticks: i32,
    ) -> bool {
        if let Some(cur) = &self.current_swing {
            if self.ticks <= cur.duration_ticks / 2 && self.ticks > 0 {
                return false;
            }
        }
        let old = self.current_swing;
        let new = SwingDescription {
            hand,
            animation,
            duration_ticks,
        };
        self.current_swing = Some(new);
        if old != Some(new) {
            self.old_animation = 0.0;
            self.animation = 0.0;
        }
        self.ticks = 0;
        true
    }

    pub fn tick(&mut self) {
        self.old_animation = self.animation;
        if let Some(cur) = self.current_swing {
            if cur.duration_ticks > 0 {
                self.animation = (self.ticks as f32 / cur.duration_ticks as f32).min(1.0);
            }
            let t = self.ticks;
            self.ticks += 1;
            if t > cur.duration_ticks {
                self.current_swing = None;
                self.animation = 0.0;
            }
        } else {
            self.animation = 0.0;
        }
    }

    pub fn get_animation(&self, partial_ticks: f32) -> f32 {
        if self.animation >= self.old_animation {
            mth::lerp_f32(partial_ticks, self.old_animation, self.animation)
        } else {
            mth::lerp_f32(partial_ticks, self.old_animation, self.animation + 1.0)
        }
    }

    /// `animation` (the swing progress of the current tick).
    pub fn animation(&self) -> f32 {
        self.animation
    }

    pub fn is_swinging(&self) -> bool {
        self.current_swing.is_some()
    }
}
