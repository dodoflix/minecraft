//! Gliding (elytra): `canGlide`, `updateFallFlying`, `stopFallFlying`.

use super::util::{self, has_effect, l};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::entity::FLAG_FALL_FLYING;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::ItemStack;
use crate::world::item::ItemStackExt;
use mc_core::random::RandomSource;
use mc_data::{GameEvent, MobEffect};
use mc_protocol::components::{EQUIPPABLE, GLIDER};

/// `LivingEntity.canGlideUsing(itemStack, slot)`.
pub fn can_glide_using(stack: &ItemStack, slot: EquipmentSlot) -> bool {
    if stack.get(&GLIDER).is_none() {
        return false;
    }
    stack
        .get(&EQUIPPABLE)
        .is_some_and(|equippable| slot == EquipmentSlot::from_protocol(equippable.slot))
        && !stack.next_damage_will_break()
}

/// `canGlide()`.
pub fn can_glide(entity: &dyn Entity) -> bool {
    if !entity.base().on_ground()
        && !entity.base().is_passenger()
        && !has_effect(entity, MobEffect::LEVITATION)
    {
        EquipmentSlot::VALUES
            .into_iter()
            .any(|slot| can_glide_using(&entity.get_item_by_slot(slot), slot))
    } else {
        false
    }
}

/// `stopFallFlying()`.
pub fn stop_fall_flying(entity: &mut dyn Entity) {
    entity.base_mut().set_shared_flag(FLAG_FALL_FLYING, true);
    entity.base_mut().set_shared_flag(FLAG_FALL_FLYING, false);
}

/// `Entity.checkFallDistanceAccumulation()`.
fn check_fall_distance_accumulation(entity: &mut dyn Entity) {
    let b = entity.base_mut();
    if b.delta_movement().y > -0.5 && b.fall_distance > 1.0 {
        b.fall_distance = 1.0;
    }
}

/// `updateFallFlying()`.
pub fn update_fall_flying(entity: &mut dyn Entity, level: &mut ServerLevel) {
    check_fall_distance_accumulation(entity);
    if !entity.can_glide() {
        entity.base_mut().set_shared_flag(FLAG_FALL_FLYING, false);
        return;
    }
    let check_fall_fly_ticks = l(entity).fall_fly_ticks + 1;
    if check_fall_fly_ticks % 10 == 0 {
        let free_fall_interval = check_fall_fly_ticks / 10;
        if free_fall_interval % 2 == 0 {
            let slots_with_gliders: Vec<EquipmentSlot> = EquipmentSlot::VALUES
                .into_iter()
                .filter(|&slot| can_glide_using(&entity.get_item_by_slot(slot), slot))
                .collect();
            // `Util.getRandom(list, random)`: `list.get(random.nextInt(list.size()))`
            let index = entity
                .base_mut()
                .random
                .next_int_bounded(slots_with_gliders.len() as i32) as usize;
            let slot_to_damage = slots_with_gliders[index];
            entity.hurt_and_break_equipment(level, slot_to_damage, 1);
        }
        util::game_event(level, entity, GameEvent::ELYTRA_GLIDE);
    }
}

/// `ItemStack.hurtAndBreak(amount, this, slot)` for a non-player owner (players: `ServerPlayer`'s override).
pub fn hurt_and_break_equipment(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    slot: EquipmentSlot,
    amount: i32,
) {
    let Some(mut stack) = entity.item_by_slot_mut(slot).map(|s| s.clone()) else {
        return;
    };
    if !stack.is_damageable_item() || amount <= 0 {
        return;
    }
    let broken = crate::world::item::hurt_and_break_stack(&mut stack, amount, level, None);
    if let Some(live) = entity.item_by_slot_mut(slot) {
        *live = stack;
    }
    if let Some(broken) = broken {
        entity.on_equipped_item_broken(level, &broken, slot);
    }
}
