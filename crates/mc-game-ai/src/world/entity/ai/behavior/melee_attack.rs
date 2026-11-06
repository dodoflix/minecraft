//! `MeleeAttack`.

use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{BehaviorBuilder, OneShot};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    ATTACK_COOLING_DOWN, ATTACK_TARGET, LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::mob::combat;
use mc_game_core::world::item::InteractionHand;

/// `create(cooldownBetweenAttacks)`.
pub fn create(cooldown_between_attacks: i32) -> OneShot {
    create_with(|_| true, cooldown_between_attacks)
}

/// `create(canAttackPredicate, cooldownBetweenAttacks)`.
pub fn create_with(
    can_attack_predicate: impl Fn(&dyn Entity) -> bool + 'static,
    cooldown_between_attacks: i32,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let look_target = i.registered(&LOOK_TARGET);
    let attack_target = i.present(&ATTACK_TARGET);
    let attack_cooling_down = i.absent(&ATTACK_COOLING_DOWN);
    let nearest_entities = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let target = c.get(&attack_target).0;
        let visible = c.get(&nearest_entities).clone();
        let (level, body) = (&mut *c.level, &mut *c.body);
        let Some(target_entity) = level.get_entity(target) else {
            return false;
        };
        let holds_usable_non_melee_weapon = [EquipmentSlot::MainHand, EquipmentSlot::OffHand]
            .into_iter()
            .any(|slot| body.can_use_non_melee_weapon(&body.get_item_by_slot(slot)));
        if can_attack_predicate(&*body)
            && !holds_usable_non_melee_weapon
            && combat::is_within_melee_attack_range(&*body, &*level, target_entity)
            && visible.contains(&*level, &*body, target)
        {
            let look = PositionTracker::Entity(EntityTracker::new(target_entity, true));
            c.set(&look_target, look);
            let (level, body) = (&mut *c.level, &mut *c.body);
            combat::swing_for_attack(body, level, InteractionHand::MainHand);
            combat::do_hurt_target(body, level, target);
            c.set_with_expiry(
                &attack_cooling_down,
                true,
                i64::from(cooldown_between_attacks),
            );
            true
        } else {
            false
        }
    })
}
