//! `LivingEntity.getCombatTracker()` calls: `recordDamage` / `recheckStatus` and the combat enter/leave hooks.

use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;

/// `getCombatTracker().recordDamage(source, damage)` (then `onLeaveCombat()` / `onEnterCombat()` as the tracker decided).
pub fn record_damage(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    source: &DamageSource,
    damage: f32,
) {
    let mut tracker = std::mem::take(&mut entity.living_mut().expect("living").combat_tracker);
    let events = tracker.record_damage(level, entity, source, damage);
    entity.living_mut().expect("living").combat_tracker = tracker;
    if events.left_combat {
        entity.on_leave_combat(level);
    }
    if events.entered_combat {
        entity.on_enter_combat(level);
    }
}

/// `getCombatTracker().recheckStatus()` (`LivingEntity.remove` / every 20 ticks of `tick()` / `ServerPlayer.die`).
pub fn recheck_status(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let mut tracker = std::mem::take(&mut entity.living_mut().expect("living").combat_tracker);
    let left_combat = tracker.recheck_status(entity);
    entity.living_mut().expect("living").combat_tracker = tracker;
    if left_combat {
        entity.on_leave_combat(level);
    }
}

// ---- attacking: `LivingEntity`/`Mob` melee helpers ---------------------------------------------------------------------------

use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::{ItemStack, enchantment_helper};
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_data::Attribute;
use mc_protocol::components::Holder;
use mc_protocol::components::values::basic::DAMAGE_TYPE;

/// `Mth.PI / 180` as the `float` the Java expressions multiply the rotation with.
pub const DEG_TO_RAD: f32 = (std::f64::consts::PI / 180.0) as f32;

/// `attacker.createDamageSource()`: `LivingEntity` = `mob_attack`, `Player` = `player_attack`.
pub fn create_damage_source(level: &ServerLevel, attacker: &dyn Entity) -> DamageSource {
    if attacker.is_player() {
        level.damage_sources().player_attack(attacker)
    } else {
        level.damage_sources().mob_attack(attacker)
    }
}

/// `ItemStack.getDamageSource(attacker)`: the `damage_type` component, else the item's, else the attacker's default.
pub fn item_stack_damage_source(
    level: &ServerLevel,
    stack: &ItemStack,
    attacker: &dyn Entity,
) -> DamageSource {
    if let Some(Holder::Reference(id)) = stack.get(&DAMAGE_TYPE)
        && let Some(data) = level
            .registries
            .get::<crate::registry::typed::damage_type::DamageType>(
                crate::registry::keys::DAMAGE_TYPE,
                id,
            )
    {
        return DamageSource::new(
            id.clone(),
            std::sync::Arc::new(data.clone()),
            Some(attacker),
            Some(attacker),
            None,
        );
    }
    crate::world::item::behaviour(stack.item())
        .get_item_damage_source(level, attacker)
        .unwrap_or_else(|| create_damage_source(level, attacker))
}

/// `LivingEntity.getKnockback(target, damageSource)`: the `attack_knockback` attribute run through the weapon's enchantments, halved.
pub fn get_knockback(
    attacker: &mut dyn Entity,
    level: &mut ServerLevel,
    target: &mut dyn Entity,
    source: &DamageSource,
) -> f32 {
    let knockback = attacker
        .living()
        .expect("living")
        .get_attribute_value(Attribute::ATTACK_KNOCKBACK) as f32;
    let weapon = attacker.get_weapon_item().unwrap_or_else(ItemStack::empty);
    let target_id = target.id();
    let mut actors = Actors::of(attacker).and(target);
    enchantment_helper::modify_knockback(level, &mut actors, &weapon, target_id, source, knockback)
        / 2.0
}

/// `LivingEntity.causeExtraKnockback`.
pub fn cause_extra_knockback(
    attacker: &mut dyn Entity,
    target: &mut dyn Entity,
    knockback: f32,
    source: &DamageSource,
    damage: f32,
    comes_from_effect: bool,
) {
    if knockback > 0.0 && target.living().is_some() {
        let y_rot = attacker.base().y_rot();
        target.knockback(
            f64::from(knockback),
            f64::from(mth::sin(f64::from(y_rot * DEG_TO_RAD))),
            f64::from(-mth::cos(f64::from(y_rot * DEG_TO_RAD))),
            source,
            damage,
            comes_from_effect,
        );
        let m = attacker.base().delta_movement();
        attacker
            .base_mut()
            .set_delta_movement(m.multiply(Vec3::new(0.6, 1.0, 0.6)));
    }
}

/// `LivingEntity.postPiercingAttack()`.
pub fn post_piercing_attack(attacker: &mut dyn Entity, level: &mut ServerLevel) {
    let id = attacker.id();
    enchantment_helper::do_post_piercing_attack_effects(level, &mut Actors::of(attacker), id);
}

/// `LivingEntity.stabAttack(weaponSlot, target, baseDamage, dealsDamage, dealsKnockback, dismounts)`: the stab of a mob; `target` is
/// taken out of the level.
pub fn stab_attack(
    level: &mut ServerLevel,
    attacker: &mut dyn Entity,
    weapon_slot: EquipmentSlot,
    target: &mut dyn Entity,
    base_damage: f32,
    deals_damage: bool,
    deals_knockback: bool,
    dismounts: bool,
) -> bool {
    let weapon_item = attacker.get_item_by_slot(weapon_slot);
    let damage_source = item_stack_damage_source(level, &weapon_item, &*attacker);
    let target_id = target.id();
    let post_enchantment_damage = enchantment_helper::modify_damage(
        level,
        &mut Actors::of(&mut *attacker).and(&mut *target),
        &weapon_item,
        target_id,
        &damage_source,
        base_damage,
    );
    let old_movement = target.base().delta_movement();
    let dealt_damage =
        deals_damage && target.hurt_server(level, &damage_source, post_enchantment_damage);
    let mut affected = deals_knockback || dealt_damage;
    if deals_knockback {
        attacker.cause_extra_knockback(
            &mut *target,
            0.4,
            old_movement,
            &damage_source,
            post_enchantment_damage,
            false,
        );
        let knockback = get_knockback(&mut *attacker, level, &mut *target, &damage_source);
        attacker.cause_extra_knockback(
            &mut *target,
            knockback,
            old_movement,
            &damage_source,
            post_enchantment_damage,
            true,
        );
    }
    if dismounts && crate::world::item::piercing_weapon::dismount(level, &mut *target) {
        affected = true;
    }
    if target.living().is_some() {
        // `weaponItem.hurtEnemy(livingTarget, this)` on the stack of the slot
        let mut stack = weapon_item;
        crate::world::item::hurt_enemy(level, &mut stack, &mut *target, &mut *attacker);
        if let Some(slot) = attacker.item_by_slot_mut(weapon_slot) {
            *slot = stack;
        }
    }
    if dealt_damage {
        enchantment_helper::do_post_attack_effects(
            level,
            &mut Actors::of(&mut *attacker).and(&mut *target),
            target_id,
            &damage_source,
        );
    }
    if !affected {
        return false;
    }
    attacker.set_last_hurt_mob(&*target);
    attacker.play_attack_sound(level);
    true
}

/// `Mob.doHurtTarget(level, target)`: the melee attack of a mob (`ATTACK_DAMAGE` + weapon enchantments + item bonus, knockback,
/// weapon `hurtEnemy`, post-attack effects). Mob classes call this from their `do_hurt_target` override.
pub fn mob_do_hurt_target(
    attacker: &mut dyn Entity,
    level: &mut ServerLevel,
    target: &mut dyn Entity,
) -> bool {
    let mut dmg = attacker
        .living()
        .expect("living")
        .get_attribute_value(Attribute::ATTACK_DAMAGE) as f32;
    let weapon_item = attacker.get_weapon_item().unwrap_or_else(ItemStack::empty);
    let damage_source = item_stack_damage_source(level, &weapon_item, &*attacker);
    let target_id = target.id();
    dmg = enchantment_helper::modify_damage(
        level,
        &mut Actors::of(&mut *attacker).and(&mut *target),
        &weapon_item,
        target_id,
        &damage_source,
        dmg,
    );
    dmg += crate::world::item::behaviour(weapon_item.item()).get_attack_damage_bonus_for(
        level,
        &weapon_item,
        &mut *target,
        dmg,
        &damage_source,
        &mut *attacker,
    );
    let old_movement = target.base().delta_movement();
    let was_hurt = target.hurt_server(level, &damage_source, dmg);
    if was_hurt {
        let knockback = get_knockback(&mut *attacker, level, &mut *target, &damage_source);
        attacker.cause_extra_knockback(
            &mut *target,
            knockback,
            old_movement,
            &damage_source,
            dmg,
            true,
        );
        if target.living().is_some() {
            // `weaponItem.hurtEnemy(livingTarget, this)` on the held stack
            let mut stack = weapon_item.clone();
            crate::world::item::hurt_enemy(level, &mut stack, &mut *target, &mut *attacker);
            if let Some(slot) = attacker.item_by_slot_mut(EquipmentSlot::MainHand) {
                *slot = stack;
            }
        }
        enchantment_helper::do_post_attack_effects(
            level,
            &mut Actors::of(&mut *attacker).and(&mut *target),
            target_id,
            &damage_source,
        );
        attacker.set_last_hurt_mob(&*target);
        attacker.play_attack_sound(level);
    }
    post_piercing_attack(attacker, level);
    was_hurt
}
