//! `MobEffect` and its subclasses (`MobEffects` anonymous/named classes), dispatched on the extracted class name of
//! the effect (`MobEffectData.class`): tick actions, tick intervals, instantaneous application, start/added/removed/
//! hurt callbacks, particles and attribute modifier templates.
//!
//! Gaps (owner = mob classes / raids): `OozingMobEffect`/`InfestedMobEffect` spawn `Slime`/`Silverfish`, which only
//! happens once those classes register an entity factory; `BadOmenMobEffect`/`RaidOmenMobEffect` need the raid
//! manager and the POI manager ([`RaidHooks`]).

use crate::server::level::ServerLevel;
use crate::world::entity::attributes::{AttributeMap, AttributeModifier, AttributeModifierOperation};
use crate::world::entity::effect::MobEffectInstance;
use crate::world::entity::{Entity, RemovalReason};
use mc_core::random::RandomSource;
use mc_data::enums::{AttributeModifierOperation as DataOperation, MobEffectCategory};
use mc_data::{Attribute, MobEffect, ParticleType};
use mc_protocol::packets::play::{ParticleOptions, SoundSource};
use super::reactions;
use std::sync::OnceLock;

/// `AMBIENT_ALPHA = Mth.floor(38.25F)`.
const AMBIENT_ALPHA: i32 = 38;

/// The `source` / `owner` entities of `applyInstantaneousEffect`: either the affected mob itself (a player drinking
/// a potion passes itself, which the borrow checker cannot express as a second reference) or another entity.
#[derive(Clone, Copy)]
pub enum EffectCause<'a> {
    Mob,
    Entity(&'a dyn Entity),
}

/// The raid / POI side of bad omen and raid omen (`ServerLevel.isVillage`, `getRaidAt`, `Raids.createOrExtendRaid`).
/// SEAM(raids): installed by the raid manager; without it the effects behave as outside a village.
pub trait RaidHooks: Send + Sync {
    /// `level.isVillage(pos)`.
    fn is_village(&self, level: &ServerLevel, pos: mc_core::pos::BlockPos) -> bool;
    /// `level.getRaidAt(pos)`: `(raidOmenLevel, maxRaidOmenLevel)` of the raid there.
    fn raid_omen_levels_at(&self, level: &ServerLevel, pos: mc_core::pos::BlockPos) -> Option<(i32, i32)>;
    /// `level.getRaids().createOrExtendRaid(player, pos)`.
    fn create_or_extend_raid(&self, level: &mut ServerLevel, player: &mut dyn Entity, pos: mc_core::pos::BlockPos);
}

static RAID_HOOKS: OnceLock<&'static dyn RaidHooks> = OnceLock::new();

pub fn set_raid_hooks(hooks: &'static dyn RaidHooks) {
    let _ = RAID_HOOKS.set(hooks);
}

fn data_operation(op: DataOperation) -> AttributeModifierOperation {
    match op {
        DataOperation::AddValue => AttributeModifierOperation::AddValue,
        DataOperation::AddMultipliedBase => AttributeModifierOperation::AddMultipliedBase,
        DataOperation::AddMultipliedTotal => AttributeModifierOperation::AddMultipliedTotal,
    }
}

/// Java `int << n` / `>> n` (the shift distance is masked to 5 bits).
fn shl(value: i32, n: i32) -> i32 {
    value.wrapping_shl(n as u32)
}
fn shr(value: i32, n: i32) -> i32 {
    value.wrapping_shr(n as u32)
}

/// Tick interval effects (`poison`, `wither`, `regeneration`): `interval = base >> amplification`.
fn interval_elapsed(base: i32, tick_count: i32, amplification: i32) -> bool {
    let interval = shr(base, amplification);
    if interval > 0 { tick_count.wrapping_rem(interval) == 0 } else { true }
}

pub trait MobEffectExt: Copy {
    fn class(self) -> &'static str;
    fn is_instantaneous(self) -> bool;
    fn is_beneficial(self) -> bool;
    fn get_color(self) -> i32;
    fn get_blend_in_duration_ticks(self) -> i32;
    fn get_blend_out_duration_ticks(self) -> i32;
    fn get_blend_out_advance_ticks(self) -> i32;
    /// `createModifiers(amplifier, consumer)`.
    fn create_modifiers(self, amplifier: i32) -> Vec<(Attribute, AttributeModifier)>;
    /// `addAttributeModifiers(attributes, amplifier)`.
    fn add_attribute_modifiers(self, attributes: &mut AttributeMap, amplifier: i32);
    /// `removeAttributeModifiers(attributes)`.
    fn remove_attribute_modifiers(self, attributes: &mut AttributeMap);
    /// `createParticleOptions(instance)`.
    fn create_particle_options(self, instance: &MobEffectInstance) -> ParticleOptions;
    /// `shouldApplyEffectTickThisTick(tickCount, amplification)`.
    fn should_apply_effect_tick_this_tick(self, tick_count: i32, amplification: i32) -> bool;
    /// `applyEffectTick(level, mob, amplification)`: false removes the effect.
    fn apply_effect_tick(self, level: &mut ServerLevel, mob: &mut dyn Entity, amplification: i32) -> bool;
    /// `applyInstantaneousEffect(level, source, owner, mob, amplification, scale)`.
    fn apply_instantaneous_effect(
        self,
        level: &mut ServerLevel,
        source: Option<EffectCause<'_>>,
        owner: Option<EffectCause<'_>>,
        mob: &mut dyn Entity,
        amplification: i32,
        scale: f64,
    );
    /// `onEffectStarted(mob, amplifier)`.
    fn on_effect_started(self, mob: &mut dyn Entity, amplifier: i32);
    /// `onEffectAdded(mob, amplifier)`.
    fn on_effect_added(self, level: &mut ServerLevel, mob: &mut dyn Entity, amplifier: i32);
    /// `onMobRemoved(level, mob, amplifier, reason)`.
    fn on_mob_removed(self, level: &mut ServerLevel, mob: &mut dyn Entity, amplifier: i32, reason: RemovalReason);
    /// `onMobHurt(level, mob, amplifier, source, damage)`.
    fn on_mob_hurt(self, level: &mut ServerLevel, mob: &mut dyn Entity, amplifier: i32);
}

impl MobEffectExt for MobEffect {
    fn class(self) -> &'static str {
        self.data().class
    }

    fn is_instantaneous(self) -> bool {
        self.data().instantaneous
    }

    fn is_beneficial(self) -> bool {
        self.data().category == MobEffectCategory::Beneficial
    }

    fn get_color(self) -> i32 {
        self.data().color
    }

    fn get_blend_in_duration_ticks(self) -> i32 {
        self.data().blend_in
    }

    fn get_blend_out_duration_ticks(self) -> i32 {
        self.data().blend_out
    }

    fn get_blend_out_advance_ticks(self) -> i32 {
        self.data().blend_out_advance
    }

    fn create_modifiers(self, amplifier: i32) -> Vec<(Attribute, AttributeModifier)> {
        self.data()
            .attribute_modifiers
            .iter()
            .map(|t| {
                // `AttributeTemplate.create`: `amount * (amplifier + 1)`
                let id = mc_core::ident::Identifier::parse(t.id).expect("valid modifier id");
                (t.attribute, AttributeModifier { id, amount: t.amount * f64::from(amplifier + 1), operation: data_operation(t.operation) })
            })
            .collect()
    }

    fn add_attribute_modifiers(self, attributes: &mut AttributeMap, amplifier: i32) {
        for (attribute, modifier) in self.create_modifiers(amplifier) {
            if let Some(instance) = attributes.get_instance(attribute) {
                instance.remove_modifier(&modifier.id);
                instance.add_permanent_modifier(modifier);
            }
        }
    }

    fn remove_attribute_modifiers(self, attributes: &mut AttributeMap) {
        for t in self.data().attribute_modifiers {
            if let Some(instance) = attributes.get_instance(t.attribute) {
                instance.remove_modifier(&mc_core::ident::Identifier::parse(t.id).expect("valid modifier id"));
            }
        }
    }

    fn create_particle_options(self, instance: &MobEffectInstance) -> ParticleOptions {
        let fixed = match self.class() {
            "InfestedMobEffect" => Some(ParticleType::INFESTED),
            "OozingMobEffect" => Some(ParticleType::ITEM_SLIME),
            "WeavingMobEffect" => Some(ParticleType::ITEM_COBWEB),
            "WindChargedMobEffect" => Some(ParticleType::SMALL_GUST),
            "RaidOmenMobEffect" => Some(ParticleType::RAID_OMEN),
            _ if self == MobEffect::TRIAL_OMEN => Some(ParticleType::TRIAL_OMEN),
            _ => None,
        };
        match fixed {
            Some(particle_type) => ParticleOptions::Simple(particle_type),
            None => {
                // `ColorParticleOption.create(ENTITY_EFFECT, ARGB.color(alpha, color))`
                let alpha = if instance.ambient { AMBIENT_ALPHA } else { 255 };
                ParticleOptions::Color { particle_type: ParticleType::ENTITY_EFFECT, color: (alpha << 24) | (self.data().color & 0xFF_FFFF) }
            }
        }
    }

    fn should_apply_effect_tick_this_tick(self, tick_count: i32, amplification: i32) -> bool {
        match self.class() {
            // `InstantaneousMobEffect`
            "HealOrHarmMobEffect" | "SaturationMobEffect" => tick_count >= 1,
            "PoisonMobEffect" => interval_elapsed(25, tick_count, amplification),
            "WitherMobEffect" => interval_elapsed(40, tick_count, amplification),
            "RegenerationMobEffect" => interval_elapsed(50, tick_count, amplification),
            "HungerMobEffect" | "AbsorptionMobEffect" | "BadOmenMobEffect" => true,
            "RaidOmenMobEffect" => tick_count == 1,
            _ => false,
        }
    }

    fn apply_effect_tick(self, level: &mut ServerLevel, mob: &mut dyn Entity, amplification: i32) -> bool {
        match self.class() {
            "HealOrHarmMobEffect" => {
                let is_harm = self == MobEffect::INSTANT_DAMAGE;
                if is_harm == mob.is_inverted_heal_and_harm(level) {
                    mob.heal(level, shl(4, amplification).max(0) as f32);
                } else {
                    let source = level.damage_sources().magic();
                    mob.hurt_server(level, &source, shl(6, amplification) as f32);
                }
                true
            }
            "PoisonMobEffect" => {
                if mob.get_health() > 1.0 {
                    let source = level.damage_sources().magic();
                    mob.hurt_server(level, &source, 1.0);
                }
                true
            }
            "WitherMobEffect" => {
                let source = level.damage_sources().wither();
                mob.hurt_server(level, &source, 1.0);
                true
            }
            "RegenerationMobEffect" => {
                if mob.get_health() < mob.get_max_health() {
                    mob.heal(level, 1.0);
                }
                true
            }
            "HungerMobEffect" => {
                if let Some(player) = mob.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>() {
                    player.cause_food_exhaustion(0.005 * (amplification + 1) as f32);
                }
                true
            }
            "SaturationMobEffect" => {
                if let Some(player) = mob.player_mut() {
                    player.food_data.eat(amplification + 1, 1.0);
                }
                true
            }
            "AbsorptionMobEffect" => mob.get_absorption_amount() > 0.0,
            "BadOmenMobEffect" => {
                if mob.is_player() && !mob.is_spectator() && level.difficulty() != mc_world::level_storage::Difficulty::Peaceful {
                    if let Some(hooks) = RAID_HOOKS.get() {
                        let pos = mob.block_position();
                        if hooks.is_village(level, pos) {
                            let below_max = hooks.raid_omen_levels_at(level, pos).is_none_or(|(omen, max)| omen < max);
                            if below_max {
                                mob.add_effect(level, MobEffectInstance::of(MobEffect::RAID_OMEN, 600, amplification), None);
                                if let Some(player) = mob.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>() {
                                    player.raid_omen_position = Some(pos);
                                }
                                return false;
                            }
                        }
                    }
                }
                true
            }
            "RaidOmenMobEffect" => {
                if mob.is_player() && !mob.is_spectator() {
                    let position = mob.as_any().downcast_ref::<crate::world::entity::ServerPlayer>().and_then(|p| p.raid_omen_position);
                    if let (Some(pos), Some(hooks)) = (position, RAID_HOOKS.get()) {
                        hooks.create_or_extend_raid(level, mob, pos);
                        if let Some(player) = mob.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>() {
                            player.raid_omen_position = None;
                        }
                        return false;
                    }
                }
                true
            }
            _ => true,
        }
    }

    fn apply_instantaneous_effect(
        self,
        level: &mut ServerLevel,
        source: Option<EffectCause<'_>>,
        owner: Option<EffectCause<'_>>,
        mob: &mut dyn Entity,
        amplification: i32,
        scale: f64,
    ) {
        if self.class() != "HealOrHarmMobEffect" {
            self.apply_effect_tick(level, mob, amplification);
            return;
        }
        let is_harm = self == MobEffect::INSTANT_DAMAGE;
        if is_harm == mob.is_inverted_heal_and_harm(level) {
            let amount = (scale * f64::from(shl(4, amplification)) + 0.5) as i32;
            mob.heal(level, amount as f32);
        } else {
            let amount = (scale * f64::from(shl(6, amplification)) + 0.5) as i32;
            let damage_source = match source {
                None => level.damage_sources().magic(),
                Some(source) => {
                    // the cause is either `mob` itself or another entity; both are only read to build the source
                    let mob_ref: &dyn Entity = &*mob;
                    let resolve = |c: EffectCause<'_>| match c {
                        EffectCause::Mob => mob_ref,
                        EffectCause::Entity(e) => e,
                    };
                    level.damage_sources().indirect_magic(resolve(source), owner.map(resolve))
                }
            };
            mob.hurt_server(level, &damage_source, amount as f32);
        }
    }

    fn on_effect_started(self, mob: &mut dyn Entity, amplifier: i32) {
        if self.class() == "AbsorptionMobEffect" {
            let amount = mob.get_absorption_amount().max((4 * (1 + amplifier)) as f32);
            mob.set_absorption_amount(amount);
        }
    }

    fn on_effect_added(self, level: &mut ServerLevel, mob: &mut dyn Entity, _amplifier: i32) {
        if let Some(sound) = self.data().sound_on_added {
            let p = mob.position();
            let source = if mob.is_player() { SoundSource::Players } else { SoundSource::Neutral };
            level.play_sound_at(None, p.x, p.y, p.z, sound, source, 1.0, 1.0);
        }
    }

    fn on_mob_removed(self, level: &mut ServerLevel, mob: &mut dyn Entity, _amplifier: i32, reason: RemovalReason) {
        if reason != RemovalReason::Killed {
            return;
        }
        match self.class() {
            "WeavingMobEffect" => reactions::weaving(level, mob),
            // `OozingMobEffect`: spawns `Slime` (size 2) offspring; needs the slime class (mobs slice)
            _ => {}
        }
    }

    fn on_mob_hurt(self, level: &mut ServerLevel, mob: &mut dyn Entity, _amplifier: i32) {
        if self.class() == "InfestedMobEffect" {
            reactions::infested(level, mob);
        }
    }
}

