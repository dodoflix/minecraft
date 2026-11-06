//! `RememberIfHoglinWasKilled`.

use super::piglin_ai::TIME_BETWEEN_HUNTS;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, HUNTED_RECENTLY};

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let attack_target = i.present(&ATTACK_TARGET);
    let hunted_recently = i.registered(&HUNTED_RECENTLY);
    i.create(move |c| {
        let target = c.get(&attack_target).0;
        if c.level
            .get_entity(target)
            .is_some_and(|t| t.get_type() == EntityType::HOGLIN && t.is_dead_or_dying())
        {
            let duration = c
                .level
                .tick_state
                .random
                .next_int_between_inclusive(TIME_BETWEEN_HUNTS.0, TIME_BETWEEN_HUNTS.1);
            c.set_with_expiry(&hunted_recently, true, i64::from(duration));
        }
        true
    })
}
