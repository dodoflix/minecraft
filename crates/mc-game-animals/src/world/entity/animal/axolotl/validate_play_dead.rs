//! `net.minecraft.world.entity.animal.axolotl.ValidatePlayDead`.

use crate::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::{HURT_BY_ENTITY, PLAY_DEAD_TICKS};

/// `ValidatePlayDead.create()`: counts `PLAY_DEAD_TICKS` down, ends the act at 0.
pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let play_dead_ticks = i.present(&PLAY_DEAD_TICKS);
    let hurt_by = i.registered(&HURT_BY_ENTITY);
    i.create(move |c| {
        let ticks = *c.get(&play_dead_ticks);
        if ticks <= 0 {
            c.erase(&play_dead_ticks);
            c.erase(&hurt_by);
            c.body_mut().brain_mut().use_default_activity();
        } else {
            c.set(&play_dead_ticks, ticks - 1);
        }
        true
    })
}
