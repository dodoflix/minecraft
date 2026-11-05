//! `LivingEntity`'s mob-effect handling: `addEffect`/`forceAddEffect`/`removeEffect`/`removeAllEffects`,
//! `tickEffects`, `onEffectAdded/Updated/Removed` (+ `refreshDirtyAttributes`/`onAttributeUpdated`),
//! `updateInvisibilityStatus`/glowing/effect particles (`updateDataBeforeSync`), `triggerOnDeathMobEffects`,
//! `canBeAffected`, absorption clamps.
//!
//! These are the `LivingEntity` base behaviours as free functions over `&mut dyn Entity`; the overridable parts are
//! `Entity` trait methods (`on_effect_added`, ...) whose defaults call the functions here and that `ServerPlayer`
//! overrides (packets, levitation, advancement trigger).

use crate::server::level::ServerLevel;
use crate::world::effect::MobEffectExt;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::entity::{FLAG_GLOWING, FLAG_INVISIBLE};
use crate::world::entity::living::{DATA_EFFECT_AMBIENCE_ID, DATA_EFFECT_PARTICLES, LivingEntityData};
use crate::world::entity::{Entity, EntityId, RemovalReason, ServerPlayer};
use mc_data::{Attribute, EntityType, MobEffect};
use mc_protocol::packets::play::{ClientboundRemoveMobEffectPacket, ParticleOptions};

/// `entity.is(EntityTypeTags.<tag>)`.
fn entity_type_in_tag(level: &ServerLevel, entity_type: EntityType, tag: &str) -> bool {
    let tag = mc_core::ident::Identifier::new("minecraft", tag).expect("valid tag");
    level.registries.is_in_tag(crate::registry::keys::ENTITY_TYPE, &entity_type.identifier(), &tag)
}

/// `LivingEntity.isInvertedHealAndHarm()`.
pub fn is_inverted_heal_and_harm(entity: &dyn Entity, level: &ServerLevel) -> bool {
    entity_type_in_tag(level, entity.get_type(), "inverted_healing_and_harm")
}

/// `LivingEntity.canBeAffected(newEffect)`.
pub fn can_be_affected(entity: &dyn Entity, level: &ServerLevel, new_effect: &MobEffectInstance) -> bool {
    let ty = entity.get_type();
    if entity_type_in_tag(level, ty, "immune_to_infested") {
        new_effect.effect != MobEffect::INFESTED
    } else if entity_type_in_tag(level, ty, "immune_to_oozing") {
        new_effect.effect != MobEffect::OOZING
    } else if !entity_type_in_tag(level, ty, "ignores_poison_and_regen") {
        true
    } else {
        new_effect.effect != MobEffect::REGENERATION && new_effect.effect != MobEffect::POISON
    }
}

/// `activeEffects.put` (a structural modification only for a new key).
fn put_effect(living: &mut LivingEntityData, effect: MobEffectInstance) -> Option<MobEffectInstance> {
    let previous = living.active_effects.insert(effect.effect, effect);
    if previous.is_none() {
        living.effects_mod_count = living.effects_mod_count.wrapping_add(1);
    }
    previous
}

/// `removeEffectNoUpdate(effect)`.
pub fn remove_effect_no_update(living: &mut LivingEntityData, effect: MobEffect) -> Option<MobEffectInstance> {
    let removed = living.active_effects.shift_remove(&effect);
    if removed.is_some() {
        living.effects_mod_count = living.effects_mod_count.wrapping_add(1);
    }
    removed
}

/// `addEffect(newEffect, source)`.
pub fn add_effect(entity: &mut dyn Entity, level: &mut ServerLevel, new_effect: MobEffectInstance, source: Option<EntityId>) -> bool {
    if entity.living().is_none() || !entity.can_be_affected(level, &new_effect) {
        return false;
    }
    let mut changed = false;
    let living = entity.living_mut().expect("living");
    let (updated, added) = match living.active_effects.get_mut(&new_effect.effect) {
        None => {
            put_effect(living, new_effect.clone());
            (None, true)
        }
        Some(effect) => (effect.update(&new_effect).then(|| effect.clone()), false),
    };
    if added {
        entity.on_effect_added(level, &new_effect, source);
        changed = true;
        new_effect.effect.on_effect_added(level, entity, new_effect.amplifier);
    } else if let Some(effect) = updated {
        entity.on_effect_updated(level, &effect, true, source);
        changed = true;
    }
    new_effect.effect.on_effect_started(entity, new_effect.amplifier);
    changed
}

/// `forceAddEffect(newEffect, source)`.
pub fn force_add_effect(entity: &mut dyn Entity, level: &mut ServerLevel, new_effect: MobEffectInstance, source: Option<EntityId>) {
    if entity.living().is_none() || !entity.can_be_affected(level, &new_effect) {
        return;
    }
    let previous = put_effect(entity.living_mut().expect("living"), new_effect.clone());
    match previous {
        None => entity.on_effect_added(level, &new_effect, source),
        Some(_) => entity.on_effect_updated(level, &new_effect, true, source),
    }
}

/// `removeEffect(effect)`.
pub fn remove_effect(entity: &mut dyn Entity, level: &mut ServerLevel, effect: MobEffect) -> bool {
    let Some(living) = entity.living_mut() else { return false };
    match remove_effect_no_update(living, effect) {
        Some(instance) => {
            entity.on_effects_removed(level, &[instance]);
            true
        }
        None => false,
    }
}

/// `removeAllEffects()`.
pub fn remove_all_effects(entity: &mut dyn Entity, level: &mut ServerLevel) -> bool {
    let Some(living) = entity.living_mut() else { return false };
    if living.active_effects.is_empty() {
        return false;
    }
    let removed: Vec<MobEffectInstance> = std::mem::take(&mut living.active_effects).into_values().collect();
    living.effects_mod_count = living.effects_mod_count.wrapping_add(1);
    entity.on_effects_removed(level, &removed);
    true
}

/// `triggerOnDeathMobEffects(level, reason)` (`LivingEntity.remove` with KILLED / DISCARDED).
pub fn trigger_on_death_mob_effects(entity: &mut dyn Entity, level: &mut ServerLevel, reason: RemovalReason) {
    let Some(living) = entity.living() else { return };
    let effects: Vec<MobEffectInstance> = living.active_effects.values().cloned().collect();
    for effect in &effects {
        effect.effect.on_mob_removed(level, entity, effect.amplifier, reason);
    }
    if let Some(living) = entity.living_mut() {
        if !living.active_effects.is_empty() {
            living.effects_mod_count = living.effects_mod_count.wrapping_add(1);
        }
        living.active_effects.clear();
    }
}

/// `tickEffects()` (server side). `ConcurrentModificationException` (a structural modification of the effect map
/// while its iterator is live, e.g. a totem clearing the effects from inside a poison tick) ends the loop silently,
/// like the `catch` in vanilla.
pub fn tick_effects(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let Some(living) = entity.living() else { return };
    let mut expected_mod_count = living.effects_mod_count;
    let mut index = 0;
    loop {
        let living = entity.living().expect("living");
        if index >= living.active_effects.len() || living.effects_mod_count != expected_mod_count {
            break;
        }
        let mut instance = living.active_effects[index].clone();
        let key = instance.effect;
        let tick_count = entity.base().tick_count;
        // `MobEffectInstance.tickServer`
        let mut remaining = false;
        let mut downgraded = false;
        let mut applied_or_skipped = false;
        if instance.has_remaining_duration() {
            let tick = if instance.is_infinite_duration() { tick_count } else { instance.duration };
            applied_or_skipped = !(key.should_apply_effect_tick_this_tick(tick, instance.amplifier) && !key.apply_effect_tick(level, entity, instance.amplifier));
        }
        if applied_or_skipped {
            // the effect tick may have replaced/removed the stored instance; vanilla keeps ticking the object it holds
            let live = entity.living_mut().expect("living").active_effects.get_mut(&key);
            let target = match live {
                Some(stored) => stored,
                None => &mut instance,
            };
            target.tick_down_duration();
            downgraded = target.downgrade_to_hidden_effect();
            remaining = target.has_remaining_duration();
            let updated = target.clone();
            if downgraded {
                entity.on_effect_updated(level, &updated, true, None);
            }
            instance = updated;
        }
        let _ = downgraded;
        if !remaining {
            if entity.living().expect("living").effects_mod_count != expected_mod_count {
                break; // `iterator.remove()` -> ConcurrentModificationException
            }
            let living = entity.living_mut().expect("living");
            living.active_effects.shift_remove(&key);
            living.effects_mod_count = living.effects_mod_count.wrapping_add(1);
            expected_mod_count = living.effects_mod_count;
            entity.on_effects_removed(level, &[instance]);
        } else {
            if instance.duration % 600 == 0 {
                entity.on_effect_updated(level, &instance, false, None);
            }
            index += 1;
        }
    }
}

/// `sendEffectToPassengers(effect)`: the `ClientboundUpdateMobEffectPacket` to every player passenger.
pub fn send_effect_to_passengers(entity: &dyn Entity, level: &ServerLevel, effect: &MobEffectInstance) {
    let packet = crate::server::players::join::mob_effect_packet(entity.id(), effect, false);
    for_each_player_passenger(entity, level, |player| player.send(&packet));
}

fn for_each_player_passenger(entity: &dyn Entity, level: &ServerLevel, mut f: impl FnMut(&ServerPlayer)) {
    for passenger in &entity.base().passengers {
        if let Some(player) = level.get_entity(*passenger).and_then(|p| p.as_any().downcast_ref::<ServerPlayer>()) {
            f(player);
        }
    }
}

/// `LivingEntity.onEffectAdded(effect, source)`.
pub fn on_effect_added(entity: &mut dyn Entity, level: &mut ServerLevel, effect: &MobEffectInstance, _source: Option<EntityId>) {
    let Some(living) = entity.living_mut() else { return };
    living.effects_dirty = true;
    effect.effect.add_attribute_modifiers(&mut living.attributes, effect.amplifier);
    send_effect_to_passengers(entity, level, effect);
}

/// `LivingEntity.onEffectUpdated(effect, doRefreshAttributes, source)`.
pub fn on_effect_updated(entity: &mut dyn Entity, level: &mut ServerLevel, effect: &MobEffectInstance, do_refresh_attributes: bool, _source: Option<EntityId>) {
    let Some(living) = entity.living_mut() else { return };
    living.effects_dirty = true;
    if do_refresh_attributes {
        effect.effect.remove_attribute_modifiers(&mut living.attributes);
        effect.effect.add_attribute_modifiers(&mut living.attributes, effect.amplifier);
        refresh_dirty_attributes(entity, level);
    }
    send_effect_to_passengers(entity, level, effect);
}

/// `LivingEntity.onEffectsRemoved(effects)`.
pub fn on_effects_removed(entity: &mut dyn Entity, level: &mut ServerLevel, effects: &[MobEffectInstance]) {
    let Some(living) = entity.living_mut() else { return };
    living.effects_dirty = true;
    for effect in effects {
        effect.effect.remove_attribute_modifiers(&mut living.attributes);
        let packet = ClientboundRemoveMobEffectPacket { entity_id: entity.id(), effect: effect.effect };
        for_each_player_passenger(entity, level, |player| player.send(&packet));
    }
    refresh_dirty_attributes(entity, level);
}

/// `refreshDirtyAttributes()`.
pub fn refresh_dirty_attributes(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let Some(living) = entity.living_mut() else { return };
    for attribute in living.attributes.take_attributes_to_update() {
        entity.on_attribute_updated(level, attribute);
    }
}

/// `LivingEntity.onAttributeUpdated(attribute)`. shortcut: the waypoint tracking of `WAYPOINT_TRANSMIT_RANGE`
/// (locator bar) is not modelled.
pub fn on_attribute_updated(entity: &mut dyn Entity, _level: &mut ServerLevel, attribute: Attribute) {
    if attribute == Attribute::MAX_HEALTH {
        let Some(living) = entity.living_mut() else { return };
        let max = living.get_max_health();
        if living.get_health() > max {
            living.set_health(max);
        }
    } else if attribute == Attribute::MAX_ABSORPTION {
        let max = entity.get_max_absorption();
        if entity.get_absorption_amount() > max {
            entity.set_absorption_amount(max);
        }
    } else if attribute == Attribute::SCALE {
        entity.refresh_dimensions();
    }
}

/// `areAllEffectsAmbient(effects)`.
pub fn are_all_effects_ambient<'a>(effects: impl IntoIterator<Item = &'a MobEffectInstance>) -> bool {
    effects.into_iter().all(|e| !(e.visible && !e.ambient))
}

/// `updateDirtyEffects()` (`updateDataBeforeSync`).
pub fn update_dirty_effects(entity: &mut dyn Entity) {
    let Some(living) = entity.living() else { return };
    if living.effects_dirty {
        update_invisibility_status(entity);
        update_glowing_status(entity);
        entity.living_mut().expect("living").effects_dirty = false;
    }
}

/// `updateInvisibilityStatus()`.
pub fn update_invisibility_status(entity: &mut dyn Entity) {
    let living = entity.living_mut().expect("living");
    if living.active_effects.is_empty() {
        remove_effect_particles(living);
        entity.base_mut().set_shared_flag(FLAG_INVISIBLE, false);
    } else {
        let invisible = living.active_effects.contains_key(&MobEffect::INVISIBILITY);
        // `updateSynchronizedMobEffectParticles`
        let particles: Vec<ParticleOptions> = living.active_effects.values().filter(|e| e.visible).map(|e| e.effect.create_particle_options(e)).collect();
        let ambient = are_all_effects_ambient(living.active_effects.values());
        living.entity.entity_data.set(DATA_EFFECT_PARTICLES, particles);
        living.entity.entity_data.set(DATA_EFFECT_AMBIENCE_ID, ambient);
        entity.base_mut().set_shared_flag(FLAG_INVISIBLE, invisible);
    }
}

/// `removeEffectParticles()`.
pub fn remove_effect_particles(living: &mut LivingEntityData) {
    living.entity.entity_data.set(DATA_EFFECT_PARTICLES, Vec::new());
}

/// `updateGlowingStatus()` with `LivingEntity.isCurrentlyGlowing()` (server side).
fn update_glowing_status(entity: &mut dyn Entity) {
    let glowing = entity.living().is_some_and(|l| l.active_effects.contains_key(&MobEffect::GLOWING)) || entity.base().has_glowing_tag;
    if entity.base().get_shared_flag(FLAG_GLOWING) != glowing {
        entity.base_mut().set_shared_flag(FLAG_GLOWING, glowing);
    }
}

/// `canBeSeenAsEnemy()` = `!isInvulnerable() && canBeSeenByAnyone()`.
pub fn can_be_seen_as_enemy(entity: &dyn Entity) -> bool {
    !entity.base().permanently_invulnerable && can_be_seen_by_anyone(entity)
}

/// `canBeSeenByAnyone()`.
pub fn can_be_seen_by_anyone(entity: &dyn Entity) -> bool {
    !entity.is_spectator() && entity.is_alive()
}
