//! `StartHuntingHoglin`.

use super::piglin_ai;
use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use mc_game_core::world::entity::ai::memory::EntityRef;
use mc_game_core::world::entity::ai::memory::memory_module_type::{ANGRY_AT, HUNTED_RECENTLY, NEAREST_VISIBLE_ADULT_PIGLINS, NEAREST_VISIBLE_HUNTABLE_HOGLIN};
use mc_game_core::world::entity::Entity;

pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let huntable = i.present(&NEAREST_VISIBLE_HUNTABLE_HOGLIN);
    let _angry_at = i.absent(&ANGRY_AT);
    let _hunted_recently = i.absent(&HUNTED_RECENTLY);
    let nearest_piglins = i.registered(&NEAREST_VISIBLE_ADULT_PIGLINS);
    i.create(move |c| {
        let piglins: Option<Vec<EntityRef>> = c.try_get(&nearest_piglins).cloned();
        let any_hunted = piglins.as_ref().is_some_and(|p| p.iter().any(|r| c.level.get_entity(r.0).is_some_and(has_hunted_recently)));
        if c.body.is_baby() || any_hunted {
            return false;
        }
        let hoglin = c.get(&huntable).0;
        let (level, body) = (&mut *c.level, &mut *c.body);
        level.with_entity(hoglin, |level, hoglin| {
            piglin_ai::set_anger_target(level, body, hoglin);
            piglin_ai::dont_kill_any_more_hoglins_for_a_while(level, body);
            piglin_ai::broadcast_anger_target(level, body, hoglin);
        });
        if let Some(piglins) = piglins {
            for EntityRef(id) in piglins {
                level.with_entity(id, |level, p| piglin_ai::dont_kill_any_more_hoglins_for_a_while(level, p));
            }
        }
        true
    })
}

/// `hasHuntedRecently(otherPiglin)`.
fn has_hunted_recently(other_piglin: &dyn Entity) -> bool {
    other_piglin.brain().has_memory_value(HUNTED_RECENTLY)
}
