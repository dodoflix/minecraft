//! `KineticWeapon`: the lunge of a spear in use (`damageEntities`, `getMotion`, `Condition.test`) and the `recentKineticEnemies`
//! bookkeeping of `LivingEntity` (`wasRecentlyStabbed`, `rememberStabbedEntity`, `stabbedEntities`).
//!
//! Not implemented: a target removed by an earlier stab of the same pass (an end crystal or a TNT minecart another one set
//! off) is skipped, where vanilla still runs `stabAttack` on the removed object (exhaustion, post-attack effects);
//! `EnderDragonPart` targets are not entities here, so the part-to-dragon redirection of the hit list has no effect;
//! `onKineticHit` (the local hit sound of the client for entity event 2) is client-only.

use super::piercing_weapon::{can_hit_entity, stab_attack};
use super::{ItemStack, behaviour};
use crate::advancements::triggers::CriteriaTriggers;
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::living_entity::damage::as_server_player;
use crate::world::entity::player::attack::get_attack_range_with;
use crate::world::entity::projectile::util::get_hit_entities_along;
use crate::world::entity::{Entity, EntityId};
use crate::world::level::collision::ClipBlock;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::Attribute;
use mc_protocol::components::values::combat::{KineticWeapon, KineticWeaponCondition};

/// `KineticWeapon.Condition.test(ticksUsed, attackerSpeed, relativeSpeed, entityFactor)`.
pub fn condition_test(
    condition: &KineticWeaponCondition,
    ticks_used: i32,
    attacker_speed: f64,
    relative_speed: f64,
    entity_factor: f64,
) -> bool {
    ticks_used <= condition.max_duration_ticks
        && attacker_speed >= f64::from(condition.min_speed) * entity_factor
        && relative_speed >= f64::from(condition.min_relative_speed) * entity_factor
}

/// `KineticWeapon.getMotion(entity)`: the known speed per second of the entity, or of the vehicle a non-player rides.
pub fn get_motion(level: &ServerLevel, entity: &dyn Entity) -> Vec3 {
    let root = if !entity.is_player() && entity.base().is_passenger() {
        entity.get_root_vehicle(level)
    } else {
        entity.id()
    };
    let known_speed = if root == entity.id() {
        entity.get_known_speed(level)
    } else {
        level.entity_manager.get_any(root).map_or_else(
            || entity.get_known_speed(level),
            |v| v.get_known_speed(level),
        )
    };
    known_speed.scale(20.0)
}

/// `LivingEntity.wasRecentlyStabbed(target, allowedTime)`.
fn was_recently_stabbed(
    level: &ServerLevel,
    attacker: &dyn Entity,
    target: EntityId,
    allowed_time: i32,
) -> bool {
    attacker
        .living()
        .and_then(|l| l.recent_kinetic_enemies.as_ref())
        .and_then(|m| m.get(&target))
        .is_some_and(|(time, _)| level.game_time() - time < i64::from(allowed_time))
}

/// `LivingEntity.rememberStabbedEntity(target)`.
fn remember_stabbed_entity(
    level: &ServerLevel,
    attacker: &mut dyn Entity,
    target: EntityId,
    target_is_living: bool,
) {
    let now = level.game_time();
    if let Some(m) = attacker
        .living_mut()
        .and_then(|l| l.recent_kinetic_enemies.as_mut())
    {
        m.insert(target, (now, target_is_living));
    }
}

/// `LivingEntity.stabbedEntities(e -> e instanceof LivingEntity)`.
fn stabbed_living_entities(attacker: &dyn Entity) -> i32 {
    attacker
        .living()
        .and_then(|l| l.recent_kinetic_enemies.as_ref())
        .map_or(0, |m| {
            m.values().filter(|(_, living)| *living).count() as i32
        })
}

/// `KineticWeapon.damageEntities(stack, ticksRemaining, livingEntity, equipmentSlot)`. `stack` is the caller's copy of the stack in
/// use: the stabs wear the weapon in its slot, so the copy is refreshed from there afterwards.
pub fn damage_entities(
    kinetic: &KineticWeapon,
    stack: &mut ItemStack,
    level: &mut ServerLevel,
    entity: &mut dyn Entity,
    ticks_remaining: i32,
    slot: EquipmentSlot,
) {
    let mut ticks_used = behaviour(stack.item()).get_use_duration(stack) - ticks_remaining;
    if ticks_used < kinetic.delay_ticks {
        return;
    }
    ticks_used -= kinetic.delay_ticks;
    let attacker_look = entity.get_look_angle();
    let attacker_speed_projection = attacker_look.dot(get_motion(level, &*entity));
    // `float actionFactor` widened to double
    let action_factor = if entity.is_player() {
        1.0
    } else {
        f64::from(0.2f32)
    };
    let attack_range = get_attack_range_with(&*entity, stack);
    let base_mob_damage = entity
        .living()
        .expect("living")
        .attributes
        .get_base_value(Attribute::ATTACK_DAMAGE);
    let hits = {
        let attacker: &dyn Entity = &*entity;
        let level: &ServerLevel = level;
        get_hit_entities_along(
            level,
            attacker,
            &attack_range,
            &|target| can_hit_entity(level, attacker, target),
            ClipBlock::Collider,
        )
        .unwrap_or_default()
    };
    let mut affected = false;
    for hit in hits {
        let other = hit.entity;
        if was_recently_stabbed(level, &*entity, other, kinetic.contact_cooldown_ticks) {
            continue;
        }
        let Some((target_motion, target_is_living)) = level
            .get_entity(other)
            .map(|t| (get_motion(level, t), t.living().is_some()))
        else {
            continue;
        };
        remember_stabbed_entity(level, entity, other, target_is_living);
        let target_speed_projection = attacker_look.dot(target_motion);
        let relative_speed = (attacker_speed_projection - target_speed_projection).max(0.0);
        let test = |c: &Option<KineticWeaponCondition>| {
            c.as_ref().is_some_and(|c| {
                condition_test(
                    c,
                    ticks_used,
                    attacker_speed_projection,
                    relative_speed,
                    action_factor,
                )
            })
        };
        let deals_dismount = test(&kinetic.dismount_conditions);
        let deals_knockback = test(&kinetic.knockback_conditions);
        let deals_damage = test(&kinetic.damage_conditions);
        if deals_dismount || deals_knockback || deals_damage {
            let damage_dealt = base_mob_damage as f32
                + mth::floor_f64(relative_speed * f64::from(kinetic.damage_multiplier)) as f32;
            affected |= level
                .with_entity(other, |level, target| {
                    stab_attack(
                        level,
                        entity,
                        slot,
                        target,
                        damage_dealt,
                        deals_damage,
                        deals_knockback,
                        deals_dismount,
                    )
                })
                .unwrap_or(false);
        }
    }
    if affected {
        level.broadcast_entity_event(entity.id(), 2);
        let stabbed = stabbed_living_entities(&*entity);
        if let Some(player) = as_server_player(entity) {
            CriteriaTriggers::SPEAR_MOBS_TRIGGER.trigger(level, player, stabbed);
        }
    }
    *stack = entity.get_item_by_slot(slot);
}
