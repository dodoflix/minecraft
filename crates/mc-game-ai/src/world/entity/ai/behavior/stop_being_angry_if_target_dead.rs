//! `StopBeingAngryIfTargetDead`.

use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::memory::memory_module_type::ANGRY_AT;

/// `create()`.
pub fn create() -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let angry_at = i.present(&ANGRY_AT);
    i.create(move |c| {
        let uuid = *c.get(&angry_at);
        let forgive = c.level.forgive_dead_players();
        let erase = c.level.get_entity_by_uuid(uuid).is_some_and(|e| {
            e.living().is_some() && e.is_dead_or_dying() && (!e.is_player() || forgive)
        });
        if erase {
            c.erase(&angry_at);
        }
        true
    })
}
