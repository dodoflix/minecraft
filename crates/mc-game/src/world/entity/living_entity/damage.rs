//! `LivingEntity` damage pipeline: `isInvulnerableTo`, `hurtServer`, `actuallyHurt`, armor/magic absorption,
//! knockback, `causeFallDamage`, hurt sounds. Free functions over `&mut dyn Entity` (any entity whose `living()` is
//! `Some`); `Player`/`ServerPlayer` overrides live in `player/damage.rs` and call these as their `super`.

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::{Attribute, EntityType, GameEvent, MobEffect, SoundEvent};
use mc_protocol::packets::play::{ClientboundDamageEventPacket, ClientboundEntityEventPacket, SoundSource};

use crate::server::level::ServerLevel;
use crate::world::damagesource::combat_rules;
use crate::world::damagesource::{DamageSource, DamageTypeTags, DamageTypes, HurtKnockbackDirection};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment_helper;

/// `Float.MAX_VALUE`-ish bound the stat awards use (`damage < 3.4028235E37F`).
const STAT_DAMAGE_LIMIT: f32 = 3.4028235E37;

/// `Level.broadcastEntityEvent(entity, event)`: to the tracking players and the entity itself.
pub fn broadcast_entity_event(level: &mut ServerLevel, id: EntityId, event_id: i8) {
    let packet = ClientboundEntityEventPacket { entity_id: id, event_id };
    level.entity_tracker.send_to_tracking_players_and_self(id, &crate::server::level::level_events::encode_packet(&packet));
}

/// `ServerLevel.broadcastDamageEvent(entity, source)`.
fn broadcast_damage_event(level: &mut ServerLevel, entity_id: EntityId, source: &DamageSource) {
    let source_type = level
        .registries
        .registry(crate::registry::keys::DAMAGE_TYPE)
        .and_then(|r| r.id_of(&source.damage_type))
        .expect("damage type registered") as i32;
    let packet = ClientboundDamageEventPacket {
        entity_id,
        source_type,
        source_cause_id: source.causing_entity.unwrap_or(-1),
        source_direct_id: source.direct_entity.unwrap_or(-1),
        source_position: source.source_position_raw(),
    };
    level.entity_tracker.send_to_tracking_players_and_self(entity_id, &crate::server::level::level_events::encode_packet(&packet));
}

/// `ServerPlayer` view of an entity (stats, connection) for the parts vanilla guards with `instanceof ServerPlayer`.
pub fn as_server_player(entity: &mut dyn Entity) -> Option<&mut crate::world::entity::ServerPlayer> {
    entity.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>()
}

/// `LivingEntity.isInvulnerableTo(level, source)`.
pub fn is_invulnerable_to(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource) -> bool {
    if entity.is_invulnerable_to_base(level, source) {
        return true;
    }
    let victim = entity.id();
    let mut causing = source.causing_snapshot(level);
    let mut actors = Actors::of(entity);
    if let Some(c) = causing.as_mut() {
        actors.push(&mut ***c);
    }
    enchantment_helper::is_immune_to_damage(level, &mut actors, victim, source)
}

/// `LivingEntity.hurtServer(level, source, damage)`.
pub fn hurt_server(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, mut damage: f32) -> bool {
    if entity.living().is_none() {
        return false;
    }
    if entity.is_invulnerable_to(level, source) {
        return false;
    }
    if entity.is_dead_or_dying() {
        return false;
    }
    if source.is_tag(level, DamageTypeTags::IS_FIRE) && entity.living().is_some_and(|l| l.active_effects.contains_key(&MobEffect::FIRE_RESISTANCE)) {
        return false;
    }
    if entity.is_sleeping() {
        stop_sleeping(entity, level);
    }
    entity.living_mut().expect("living").no_action_time = 0;
    if damage < 0.0 {
        damage = 0.0;
    }

    let item_in_use = entity.living().expect("living").use_item.clone();
    let damage_blocked = super::blocking::apply_item_blocking(entity, level, source, damage);
    damage -= damage_blocked;
    let blocked = damage_blocked > 0.0;
    if source.is_tag(level, DamageTypeTags::IS_FREEZING) && mc_data::tags::get::<EntityType>("freeze_hurts_extra_types").is_some_and(|t| t.contains(entity.get_type())) {
        damage *= 5.0;
    }
    if source.is_tag(level, DamageTypeTags::DAMAGES_HELMET) && !entity.get_item_by_slot(EquipmentSlot::Head).is_empty() {
        entity.hurt_helmet(level, source, damage);
        damage *= 0.75;
    }
    if damage.is_nan() || damage.is_infinite() {
        damage = f32::MAX;
    }

    let mut took_full_damage = true;
    {
        let l = entity.living().expect("living");
        if l.damage_cooldown_time as f32 > 10.0 && !source.is_tag(level, DamageTypeTags::BYPASSES_COOLDOWN) {
            let last_hurt = l.last_hurt;
            if damage <= last_hurt {
                return false;
            }
            entity.actually_hurt(level, source, damage - last_hurt);
            entity.living_mut().expect("living").last_hurt = damage;
            took_full_damage = false;
        } else {
            let l = entity.living_mut().expect("living");
            l.last_hurt = damage;
            l.damage_cooldown_time = 20;
            entity.actually_hurt(level, source, damage);
            let l = entity.living_mut().expect("living");
            l.hurt_duration = 10;
            l.hurt_time = l.hurt_duration;
        }
    }

    // [Villagers] the subclass part of `setLastHurtByMob(livingSource)` (reputation event), before the field is set
    if let Some(causing) = source.causing_info().filter(|c| c.is_living) {
        let no_anger = source.is_tag(level, DamageTypeTags::NO_ANGER)
            || (source.is(DamageTypes::WIND_CHARGE) && mc_data::tags::get::<EntityType>("no_anger_from_wind_charge").is_some_and(|t| t.contains(entity.get_type())));
        if !no_anger {
            entity.on_set_last_hurt_by_mob(level, causing.uuid, causing.is_player);
        }
    }
    resolve_mob_responsible_for_damage(entity, level, source);
    resolve_player_responsible_for_damage(entity, source);
    if took_full_damage {
        let blocks_attacks = item_in_use.get(&mc_protocol::components::values::combat::BLOCKS_ATTACKS);
        match blocks_attacks {
            Some(ba) if blocked => super::blocking::on_blocked(level, entity, ba),
            _ => broadcast_damage_event(level, entity.id(), source),
        }
        if !source.is_tag(level, DamageTypeTags::NO_IMPACT) && (!blocked || damage > 0.0) {
            entity.base_mut().sync_velocity = true; // markHurt()
        }
        let fully_blocked = blocked && damage <= 0.0;
        if !source.is_tag(level, DamageTypeTags::NO_KNOCKBACK) && !fully_blocked {
            deal_default_knockback(entity, level, source, damage, blocked);
        }
    }

    if entity.is_dead_or_dying() {
        if !check_totem_death_protection(entity, level, source) {
            if took_full_damage {
                if let Some(sound) = entity.get_death_sound() {
                    make_sound(entity, level, sound);
                }
                play_secondary_hurt_sound(entity, level, source);
            }
            entity.die(level, source);
        }
    } else if took_full_damage {
        if let Some(sound) = entity.get_hurt_sound_in(level, source) {
            make_sound(entity, level, sound);
        }
        play_secondary_hurt_sound(entity, level, source);
    }

    let success = !blocked || damage > 0.0;
    if success {
        let game_time = level.game_time();
        let l = entity.living_mut().expect("living");
        l.last_damage_source = Some(source.clone());
        l.last_damage_stamp = game_time;
        // `effect.onMobHurt(level, this, source, damage)` for every active effect (EcEffects).
        crate::world::entity::living_entity::effects::on_mob_hurt(entity, level);
    }

    if let Some(player) = as_server_player(entity) {
        crate::advancements::triggers::CriteriaTriggers::ENTITY_HURT_PLAYER.trigger(level, player, source, damage, damage, blocked);
        if damage_blocked > 0.0 && damage_blocked < STAT_DAMAGE_LIMIT {
            player.award_stat(level, crate::stats::stats::Stats::DAMAGE_BLOCKED_BY_SHIELD, mc_core::math::java::round_f32(damage_blocked * 10.0));
        }
    }
    if let Some(source_player) = source.get_entity().filter(|_| source.causing_is_player()) {
        let source = source.clone();
        crate::advancements::triggers::fire_with_entity(level, source_player, &*entity, move |level, player, victim| {
            crate::advancements::triggers::CriteriaTriggers::PLAYER_HURT_ENTITY.matches(level, player, victim, &source, damage, damage, blocked)
        });
    }
    success
}

/// `LivingEntity.stopSleeping()`.
fn stop_sleeping(entity: &mut dyn Entity, level: &mut ServerLevel) {
    super::sleeping::stop_sleeping(entity, level);
}

/// `LivingEntity.dealDefaultKnockback(source, damage, blocked)`.
pub fn deal_default_knockback(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, damage: f32, blocked: bool) {
    let mut xd = 0.0;
    let mut zd = 0.0;
    let direct = source.direct_info();
    if let Some(dir) = direct.and_then(|d| d.projectile_hurt_knockback.map(|k| (d, k))) {
        let (projectile, kind) = dir;
        let (dx, dz) = match kind {
            HurtKnockbackDirection::DeltaMovement { x, z } => (x, z),
            HurtKnockbackDirection::AwayFromProjectile => (entity.position().x - projectile.position.x, entity.position().z - projectile.position.z),
        };
        xd = -dx;
        zd = -dz;
    } else if let Some(pos) = source.get_source_position() {
        xd = pos.x - entity.position().x;
        zd = pos.z - entity.position().z;
    }
    entity.knockback(0.4f32 as f64, xd, zd, source, damage, false);
    if !blocked {
        entity.indicate_damage(level, xd, zd);
    }
}

/// `LivingEntity.knockback(power, xd, zd, source, damage, comesFromEffect)`.
pub fn knockback(entity: &mut dyn Entity, mut power: f64, mut xd: f64, mut zd: f64, _source: &DamageSource, _damage: f32, _comes_from_effect: bool) {
    let resistance = entity.living().expect("living").get_attribute_value(Attribute::KNOCKBACK_RESISTANCE);
    power *= 1.0 - resistance;
    if !(power <= 0.0) {
        entity.base_mut().needs_sync = true;
        let delta_movement = entity.base().delta_movement();
        while xd * xd + zd * zd < f64::from(1.0E-5f32) {
            let random = &mut entity.base_mut().random;
            xd = (random.next_double() - random.next_double()) * 0.01;
            zd = (random.next_double() - random.next_double()) * 0.01;
        }
        let delta_vector = Vec3::new(xd, 0.0, zd).normalize().scale(power);
        let y = if entity.base().on_ground() { (0.4f64).min(delta_movement.y / 2.0 + power) } else { delta_movement.y };
        entity.base_mut().set_delta_movement_xyz(delta_movement.x / 2.0 - delta_vector.x, y, delta_movement.z / 2.0 - delta_vector.z);
    }
}

/// `LivingEntity.resolveMobResponsibleForDamage(source)`.
pub fn resolve_mob_responsible_for_damage(entity: &mut dyn Entity, level: &ServerLevel, source: &DamageSource) {
    let Some(causing) = source.causing_info().filter(|c| c.is_living) else { return };
    if source.is_tag(level, DamageTypeTags::NO_ANGER) {
        return;
    }
    let from_wind_charge = source.is(DamageTypes::WIND_CHARGE) && mc_data::tags::get::<EntityType>("no_anger_from_wind_charge").is_some_and(|t| t.contains(entity.get_type()));
    if from_wind_charge {
        return;
    }
    // `setLastHurtByMob(livingSource)`
    let tick_count = entity.base().tick_count;
    let l = entity.living_mut().expect("living");
    l.last_hurt_by_mob = Some(causing.uuid);
    l.last_hurt_by_mob_timestamp = tick_count;
}

/// `LivingEntity.resolvePlayerResponsibleForDamage(source)`.
pub fn resolve_player_responsible_for_damage(entity: &mut dyn Entity, source: &DamageSource) {
    let Some(causing) = source.causing_info() else { return };
    let l = entity.living_mut().expect("living");
    if causing.is_player {
        l.last_hurt_by_player = Some(causing.uuid);
        l.last_hurt_by_player_memory_time = 100;
    } else if let Some(owner) = causing.tamed_wolf_owner {
        match owner {
            Some(owner) => {
                l.last_hurt_by_player = Some(owner);
                l.last_hurt_by_player_memory_time = 100;
            }
            None => {
                l.last_hurt_by_player = None;
                l.last_hurt_by_player_memory_time = 0;
            }
        }
    }
}

/// `LivingEntity.checkTotemDeathProtection(killingDamage)`.
fn check_totem_death_protection(entity: &mut dyn Entity, level: &mut ServerLevel, killing_damage: &DamageSource) -> bool {
    super::totem::check_totem_death_protection(entity, level, killing_damage)
}

/// `LivingEntity.playSecondaryHurtSound(source)`.
fn play_secondary_hurt_sound(entity: &dyn Entity, level: &mut ServerLevel, source: &DamageSource) {
    if source.is(DamageTypes::THORNS) {
        let sound_source = if entity.is_player() { SoundSource::Players } else { SoundSource::Hostile };
        let p = entity.position();
        level.play_sound_at(None, p.x, p.y, p.z, SoundEvent::ENCHANT_THORNS_HIT, sound_source, 1.0, 1.0);
    }
}

/// `LivingEntity.getVoicePitch()`.
pub fn voice_pitch(entity: &mut dyn Entity) -> f32 {
    let baby = entity.is_baby();
    let random = &mut entity.base_mut().random;
    if baby {
        (random.next_float() - random.next_float()) * 0.2 + 1.5
    } else {
        (random.next_float() - random.next_float()) * 0.2 + 1.0
    }
}

/// `LivingEntity.makeSound(sound)`.
pub fn make_sound(entity: &mut dyn Entity, level: &mut ServerLevel, sound: SoundEvent) {
    let volume = entity.get_sound_volume();
    let pitch = entity.get_voice_pitch();
    entity.play_sound(level, sound, volume, pitch);
}

/// `LivingEntity.getDamageAfterArmorAbsorb(source, damage)`.
pub fn get_damage_after_armor_absorb(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, mut damage: f32) -> f32 {
    if !source.is_tag(level, DamageTypeTags::BYPASSES_ARMOR) {
        entity.hurt_armor(level, source, damage);
        let toughness = entity.living().expect("living").get_attribute_value(Attribute::ARMOR_TOUGHNESS) as f32;
        let armor = entity.get_armor_value() as f32;
        damage = combat_rules::get_damage_after_absorb(level, entity, damage, source, armor, toughness);
    }
    damage
}

/// `LivingEntity.getDamageAfterMagicAbsorb(source, damage)`.
pub fn get_damage_after_magic_absorb(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, mut damage: f32) -> f32 {
    if source.is_tag(level, DamageTypeTags::BYPASSES_EFFECTS) {
        return damage;
    }
    let resistance = entity.living().expect("living").active_effects.get(&MobEffect::RESISTANCE).map(|e| e.amplifier);
    if let Some(amplifier) = resistance {
        if !source.is_tag(level, DamageTypeTags::BYPASSES_RESISTANCE) {
            let absorb_value = (amplifier + 1) * 5;
            let absorb = 25 - absorb_value;
            let v = damage * absorb as f32;
            let old_damage = damage;
            damage = (v / 25.0).max(0.0);
            let damage_resisted = old_damage - damage;
            if damage_resisted > 0.0 && damage_resisted < STAT_DAMAGE_LIMIT {
                if let Some(player) = as_server_player(entity) {
                    player.award_stat(level, crate::stats::stats::Stats::DAMAGE_RESISTED, mc_core::math::java::round_f32(damage_resisted * 10.0));
                } else if source.causing_is_player() {
                    let causing = source.causing_entity.expect("causing");
                    level.with_entity_or_defer(causing, move |level, e| {
                        if let Some(p) = e.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>() {
                            p.award_stat(level, crate::stats::stats::Stats::DAMAGE_DEALT_RESISTED, mc_core::math::java::round_f32(damage_resisted * 10.0));
                        }
                    });
                }
            }
        }
    }
    if damage <= 0.0 {
        return 0.0;
    }
    if source.is_tag(level, DamageTypeTags::BYPASSES_ENCHANTMENTS) {
        return damage;
    }
    let enchantment_armor = {
        let victim = entity.id();
        let mut causing = source.causing_snapshot(level);
        let mut actors = Actors::of(&mut *entity);
        if let Some(c) = causing.as_mut() {
            actors.push(&mut ***c);
        }
        enchantment_helper::get_damage_protection(level, &mut actors, victim, source)
    };
    if enchantment_armor > 0.0 {
        damage = combat_rules::get_damage_after_magic_absorb(damage, enchantment_armor);
    }
    damage
}

/// `LivingEntity.actuallyHurt(level, source, dmg)`.
pub fn actually_hurt(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, dmg: f32) {
    if entity.is_invulnerable_to(level, source) {
        return;
    }
    let mut dmg = get_damage_after_armor_absorb(entity, level, source, dmg);
    dmg = get_damage_after_magic_absorb(entity, level, source, dmg);
    let var10 = (dmg - entity.get_absorption_amount()).max(0.0);
    let absorption = entity.get_absorption_amount();
    entity.set_absorption_amount(absorption - (dmg - var10));
    let absorbed_damage = dmg - var10;
    if absorbed_damage > 0.0 && absorbed_damage < STAT_DAMAGE_LIMIT && source.causing_is_player() {
        let causing = source.causing_entity.expect("causing");
        level.with_entity_or_defer(causing, move |level, e| {
            if let Some(p) = e.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>() {
                p.award_stat(level, crate::stats::stats::Stats::DAMAGE_DEALT_ABSORBED, mc_core::math::java::round_f32(absorbed_damage * 10.0));
            }
        });
    }
    if var10 != 0.0 {
        super::combat::record_damage(entity, level, source, var10);
        let health = entity.get_health();
        entity.living_mut().expect("living").set_health(health - var10);
        let absorption = entity.get_absorption_amount();
        entity.set_absorption_amount(absorption - var10);
        entity.game_event(level, GameEvent::ENTITY_DAMAGE);
    }
}

/// `LivingEntity.doHurtEquipment(damageSource, damage, slots...)`.
pub fn do_hurt_equipment(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource, damage: f32, slots: &[EquipmentSlot]) {
    if damage <= 0.0 {
        return;
    }
    let durability_damage = f32::max(1.0, damage / 4.0) as i32;
    for &slot in slots {
        let stack = entity.get_item_by_slot(slot);
        let damage_on_hurt = stack.get(&mc_protocol::components::values::combat::EQUIPPABLE).is_some_and(|e| e.damage_on_hurt);
        if damage_on_hurt && stack.is_damageable_item() && can_be_hurt_by(&stack, level, source) {
            entity.hurt_and_break_equipment(level, slot, durability_damage);
        }
    }
}

/// `ItemStack.canBeHurtBy(source)`: the `damage_resistant` component.
fn can_be_hurt_by(stack: &crate::world::item::ItemStack, level: &ServerLevel, source: &DamageSource) -> bool {
    stack.get(&mc_protocol::components::values::combat::DAMAGE_RESISTANT).is_none_or(|r| !source.is_in_holder_set(level, &r.types))
}

/// `LivingEntity.causeFallDamage(fallDistance, damageModifier, damageSource)`.
pub fn cause_fall_damage(entity: &mut dyn Entity, level: &mut ServerLevel, fall_distance: f64, damage_modifier: f32, source: &DamageSource) -> bool {
    if entity.living().is_none() {
        return crate::world::entity::entity::cause_fall_damage_base(entity, level, fall_distance, damage_modifier, source);
    }
    let effective_fall_distance;
    let impact = entity.living().expect("living").current_impulse_impact_pos;
    if let Some(impact) = impact {
        effective_fall_distance = fall_distance.min(impact.y - entity.base().y());
        let has_landed_above_current_impulse_impact_pos_y = effective_fall_distance <= 0.0;
        if has_landed_above_current_impulse_impact_pos_y {
            reset_current_impulse_context(entity);
        } else {
            try_reset_current_impulse_context(entity);
        }
    } else {
        effective_fall_distance = fall_distance;
    }

    let damaged = crate::world::entity::entity::cause_fall_damage_base(entity, level, effective_fall_distance, damage_modifier, source);
    let dmg = entity.calculate_fall_damage(effective_fall_distance, damage_modifier);
    if dmg > 0 {
        reset_current_impulse_context(entity);
        let sound = fall_damage_sound(entity, dmg);
        entity.play_sound(level, sound, 1.0, 1.0);
        play_block_fall_sound(entity, level);
        entity.hurt_server(level, source, dmg as f32);
        true
    } else {
        damaged
    }
}

/// `getFallDamageSound(dmg)`.
fn fall_damage_sound(entity: &dyn Entity, dmg: i32) -> SoundEvent {
    let (small, big) = entity.get_fall_sounds();
    if dmg > 4 { big } else { small }
}

/// `LivingEntity.calculateFallDamage(fallDistance, damageModifier)`.
pub fn calculate_fall_damage(entity: &dyn Entity, fall_distance: f64, damage_modifier: f32) -> i32 {
    if mc_data::tags::get::<EntityType>("fall_damage_immune").is_some_and(|t| t.contains(entity.get_type())) {
        return 0;
    }
    let living = entity.living().expect("living");
    let base_damage = fall_distance + 1.0E-6 - living.get_attribute_value(Attribute::SAFE_FALL_DISTANCE);
    mth::floor_f64(base_damage * f64::from(damage_modifier) * living.get_attribute_value(Attribute::FALL_DAMAGE_MULTIPLIER))
}

/// `LivingEntity.playBlockFallSound()`.
pub fn play_block_fall_sound(entity: &mut dyn Entity, level: &mut ServerLevel) {
    use crate::world::level::block::state::BlockStateExt as _;
    use crate::world::level::level_access::BlockGetter as _;
    if entity.base().is_silent() {
        return;
    }
    let p = entity.position();
    let pos = mc_core::pos::BlockPos::new(mth::floor_f64(p.x), mth::floor_f64(p.y - f64::from(0.2f32)), mth::floor_f64(p.z));
    let state = level.get_block_state(pos);
    if !state.is_air() {
        let sound_type = state.get_sound_type();
        entity.play_sound(level, sound_type.fall_sound, sound_type.volume * 0.5, sound_type.pitch * 0.75);
    }
}

/// `resetCurrentImpulseContext()`.
pub fn reset_current_impulse_context(entity: &mut dyn Entity) {
    let l = entity.living_mut().expect("living");
    l.current_impulse_context_reset_grace_time = 0;
    l.current_impulse_impact_pos = None;
}

/// `tryResetCurrentImpulseContext()`.
fn try_reset_current_impulse_context(entity: &mut dyn Entity) {
    if entity.living().expect("living").current_impulse_context_reset_grace_time == 0 {
        reset_current_impulse_context(entity);
    }
}

/// `LivingEntity.die(source)`.
pub fn die(entity: &mut dyn Entity, level: &mut ServerLevel, source: &DamageSource) {
    super::death::die(entity, level, source);
}
