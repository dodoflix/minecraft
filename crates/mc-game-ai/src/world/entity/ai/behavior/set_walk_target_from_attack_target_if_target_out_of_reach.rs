//! `SetWalkTargetFromAttackTargetIfTargetOutOfReach`.

use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::behavior::behavior_utils;
use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_TARGET, LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES, WALK_TARGET,
};
use mc_game_core::world::entity::entity::Entity;

/// `PROJECTILE_ATTACK_RANGE_BUFFER`.
const PROJECTILE_ATTACK_RANGE_BUFFER: i32 = 1;

/// `create(speedModifier)`.
pub fn create(speed_modifier: f32) -> OneShot {
    create_with(move |_| speed_modifier)
}

/// `create(speedModifier function)`.
pub fn create_with(speed_modifier: impl Fn(&dyn Entity) -> f32 + 'static) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.registered(&WALK_TARGET);
    let look_target = i.registered(&LOOK_TARGET);
    let attack_target = i.present(&ATTACK_TARGET);
    let nearest_entities = i.registered(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let to_attack = c.get(&attack_target).0;
        let within_reach = c
            .try_get(&nearest_entities)
            .is_some_and(|e| e.contains(&*c.level, &*c.body, to_attack))
            && behavior_utils::is_within_attack_range(
                &*c.level,
                &*c.body,
                to_attack,
                PROJECTILE_ATTACK_RANGE_BUFFER,
            );
        if within_reach {
            c.erase(&walk_target);
        } else {
            let Some(target) = c.level.get_entity_or_recently_removed(to_attack) else {
                return true;
            };
            let look = EntityTracker::new(target, true);
            let walk = EntityTracker::new(target, false);
            c.set(&look_target, PositionTracker::Entity(look));
            let speed = speed_modifier(&*c.body);
            c.set(
                &walk_target,
                WalkTarget::new(PositionTracker::Entity(walk), speed, 0),
            );
        }
        true
    })
}
