//! Methods of vanilla component records that the protocol crate does not carry (they need game types or are pure
//! gameplay logic).
use mc_protocol::components::Consumable;

pub trait ConsumableExt {
    /// `consumeTicks()` = `(int)(consumeSeconds * 20.0F)`.
    fn consume_ticks(&self) -> i32;
    /// `shouldEmitParticlesAndSounds(useItemRemainingTicks)`.
    fn should_emit_particles_and_sounds(&self, use_item_remaining_ticks: i32) -> bool;
}

impl ConsumableExt for Consumable {
    fn consume_ticks(&self) -> i32 {
        (self.consume_seconds * 20.0f32) as i32
    }

    fn should_emit_particles_and_sounds(&self, use_item_remaining_ticks: i32) -> bool {
        let used_for = self.consume_ticks() - use_item_remaining_ticks;
        let wait_ticks_before_use_effects = (self.consume_ticks() as f32 * 0.21875f32) as i32;
        used_for > wait_ticks_before_use_effects && use_item_remaining_ticks % 4 == 0
    }
}
