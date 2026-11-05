//! Application of the entity / location-based effects: `EnchantmentEntityEffect.apply`, `onChangedBlock`, `onDeactivated`.
//!
//! Every effect addresses its target by [`EntityId`] and borrows it through [`Actors`] only while it runs.

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::GameEvent;
use mc_data::block::BlockState;

use super::{
    ApplyEntityImpulse, ApplyMobEffect, DamageEntity, EnchantmentEntityEffect, EnchantmentLocationBasedEffect, PlaySoundEffect, ReplaceBlock, ReplaceDisk,
    SetBlockProperties,
};
use crate::registry::typed::damage_type::DamageType;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::EntityId;
use crate::world::item::enchantment::actors::Actors;
use crate::world::item::enchantment::enchanted_item_in_use::{EnchantedItemInUse, OnBreak};
use crate::world::level::{BlockGetter, LevelAccessor};
use crate::world::level::gameevent::GameEventContext;
use crate::world::entity::effect::MobEffectInstance;
use mc_protocol::components::{Holder, RegistrySet};

/// `EnchantmentEntityEffect.apply(level, enchantmentLevel, item, entity, position)`.
pub fn apply(
    effect: &EnchantmentEntityEffect,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    enchantment_level: i32,
    item: &mut EnchantedItemInUse,
    entity: EntityId,
    position: Vec3,
) {
    use EnchantmentEntityEffect as E;
    match effect {
        E::AllOf(effects) => {
            for e in effects {
                apply(e, level, actors, enchantment_level, item, entity, position);
            }
        }
        E::ApplyMobEffect(e) => apply_mob_effect(e, level, actors, enchantment_level, entity),
        E::ChangeItemDamage { amount } => change_item_damage(level, actors, item, amount.calculate(enchantment_level) as i32),
        E::DamageEntity(e) => damage_entity(e, level, actors, enchantment_level, item, entity),
        E::Ignite { duration } => {
            let seconds = duration.calculate(enchantment_level);
            actors.with(level, entity, |_, e| e.ignite_for_seconds(seconds));
        }
        E::ApplyImpulse(e) => apply_impulse(e, level, actors, enchantment_level, entity),
        E::PlaySound(e) => play_sound(e, level, actors, enchantment_level, entity, position),
        E::ReplaceBlock(e) => replace_block(e, level, actors, entity, position),
        E::ReplaceDisk(e) => replace_disk(e, level, actors, enchantment_level, entity, position),
        E::SetBlockProperties(e) => set_block_properties(e, level, actors, entity, position),
        E::Explode(_) | E::ApplyExhaustion { .. } | E::RunFunction { .. } | E::SpawnParticles(_) | E::SummonEntity(_) => {
            super::deferred::apply(effect, level, actors, enchantment_level, item, entity, position)
        }
    }
}

/// `EnchantmentLocationBasedEffect.onChangedBlock(...)`.
pub fn on_changed_block(
    effect: &EnchantmentLocationBasedEffect,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    enchantment_level: i32,
    item: &mut EnchantedItemInUse,
    entity: EntityId,
    position: Vec3,
    became_active: bool,
) {
    match effect {
        EnchantmentLocationBasedEffect::AllOf(effects) => {
            for e in effects {
                on_changed_block(e, level, actors, enchantment_level, item, entity, position, became_active);
            }
        }
        EnchantmentLocationBasedEffect::Attribute(a) => {
            if became_active {
                if let Some(slot) = item.in_slot {
                    let modifier = a.get_modifier(enchantment_level, slot.get_name());
                    actors.with(level, entity, |_, e| {
                        if let Some(living) = e.living_mut() {
                            living.attributes.add_transient_attribute_modifiers(&[(a.attribute, modifier)]);
                        }
                    });
                }
            }
        }
        // `EnchantmentEntityEffect.onChangedBlock` = `apply`
        EnchantmentLocationBasedEffect::Entity(e) => apply(e, level, actors, enchantment_level, item, entity, position),
    }
}

/// `EnchantmentLocationBasedEffect.onDeactivated(item, entity, position, level)`.
pub fn on_deactivated(
    effect: &EnchantmentLocationBasedEffect,
    level: &mut ServerLevel,
    actors: &mut Actors<'_>,
    item: &EnchantedItemInUse,
    entity: EntityId,
    position: Vec3,
    enchantment_level: i32,
) {
    match effect {
        EnchantmentLocationBasedEffect::AllOf(effects) => {
            for e in effects {
                on_deactivated(e, level, actors, item, entity, position, enchantment_level);
            }
        }
        EnchantmentLocationBasedEffect::Attribute(a) => {
            if let Some(slot) = item.in_slot {
                let modifier = a.get_modifier(enchantment_level, slot.get_name());
                actors.with(level, entity, |_, e| {
                    if let Some(living) = e.living_mut() {
                        living.attributes.remove_attribute_modifiers(&[(a.attribute, modifier)]);
                    }
                });
            }
        }
        // entity effects keep the default no-op `onDeactivated`
        EnchantmentLocationBasedEffect::Entity(_) => {}
    }
}

// ---- individual effects -------------------------------------------------------------------------------------------------------------

/// `RegistrySet<MobEffect>.getRandomElement(random)`: `Util.getRandomSafe(contents, random)`.
fn random_mob_effect(registries: &crate::registry::RegistryAccess, set: &RegistrySet<mc_data::MobEffect>, random: &mut dyn RandomSource) -> Option<mc_data::MobEffect> {
    let contents: Vec<mc_data::MobEffect> = match set {
        RegistrySet::Entries(v) => v.clone(),
        RegistrySet::Tag(tag) => registries
            .tag("minecraft:mob_effect", tag)
            .map(|ids| ids.iter().filter_map(|i| mc_data::MobEffect::from_id(*i)).collect())
            .unwrap_or_default(),
    };
    if contents.is_empty() {
        None
    } else {
        Some(contents[random.next_int_bounded(contents.len() as i32) as usize])
    }
}

fn apply_mob_effect(e: &ApplyMobEffect, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, entity: EntityId) {
    if !actors.get(level, entity).is_some_and(|t| t.living().is_some()) {
        return;
    }
    let registries = level.registries.clone();
    let picked = actors.with_random(level, entity, |random| {
        let selected = random_mob_effect(&registries, &e.to_apply, random)?;
        let ticks = mc_core::math::java::round_f32(
            mth::random_between(random, e.min_duration.calculate(enchantment_level), e.max_duration.calculate(enchantment_level)) * 20.0,
        );
        let amplifier = mc_core::math::java::round_f32(mth::random_between(random, e.min_amplifier.calculate(enchantment_level), e.max_amplifier.calculate(enchantment_level))).max(0);
        Some((selected, ticks, amplifier))
    });
    if let Some((effect, ticks, amplifier)) = picked {
        let instance = MobEffectInstance::new(effect, ticks, amplifier, false, true, true, None);
        actors.with(level, entity, |lvl, t| t.add_effect(lvl, instance, None));
    }
}

/// `ChangeItemDamage.apply`: `itemStack.hurtAndBreak(change, level, player, onBreak)`.
fn change_item_damage(level: &mut ServerLevel, actors: &mut Actors<'_>, item: &mut EnchantedItemInUse, change: i32) {
    use mc_data::DataComponentType as C;
    if !(item.item_stack.has(C::MAX_DAMAGE) && item.item_stack.has(C::DAMAGE)) {
        return;
    }
    // `item.owner() instanceof ServerPlayer`: its creative mode makes the stack unbreakable
    let player_owner = item.owner.filter(|&o| actors.get(level, o).is_some_and(|e| e.is_player()));
    item.modified = true;
    let broken = match player_owner {
        Some(owner) => {
            let stack = &mut item.item_stack;
            actors
                .with(level, owner, |lvl, e| crate::world::item::hurt_and_break_stack(stack, change, lvl, e.as_any_mut().downcast_mut::<crate::world::entity::ServerPlayer>()))
                .flatten()
        }
        None => crate::world::item::hurt_and_break_stack(&mut item.item_stack, change, level, None),
    };
    if let Some(broken) = broken {
        match &mut item.on_break {
            OnBreak::OwnerEquipped => {
                if let (Some(owner), Some(slot)) = (item.owner, item.in_slot) {
                    actors.with(level, owner, |lvl, o| o.on_equipped_item_broken(lvl, &broken, slot));
                }
            }
            OnBreak::Callback(callback) => callback(level, &broken),
        }
    }
}

/// The `DamageType` behind a holder (`Holder.value()`), shared into a `DamageSource`.
pub(super) fn damage_type_of(level: &ServerLevel, holder: &Holder<DamageType>) -> Option<(mc_core::ident::Identifier, std::sync::Arc<DamageType>)> {
    match holder {
        Holder::Reference(id) => level.registries.get::<DamageType>("minecraft:damage_type", id).map(|d| (id.clone(), std::sync::Arc::new(d.clone()))),
        // an inline definition has no registry key; vanilla's `DamageSource` keeps the holder itself
        Holder::Direct(d) => Some((mc_core::ident::Identifier::parse("minecraft:generic").expect("valid id"), std::sync::Arc::new(d.clone()))),
    }
}

/// `new DamageSource(damageType, item.owner())`.
pub fn damage_source_of_owner(level: &ServerLevel, actors: &Actors<'_>, holder: &Holder<DamageType>, owner: Option<EntityId>) -> Option<DamageSource> {
    let (id, data) = damage_type_of(level, holder)?;
    let owner = owner.and_then(|o| actors.get(level, o));
    Some(DamageSource::new(id, data, owner, owner, None))
}

fn damage_entity(e: &DamageEntity, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, item: &EnchantedItemInUse, entity: EntityId) {
    let damage = actors.with_random(level, entity, |r| mth::random_between(r, e.min_damage.calculate(enchantment_level), e.max_damage.calculate(enchantment_level)));
    if let Some(source) = damage_source_of_owner(level, actors, &e.damage_type, item.owner) {
        actors.with(level, entity, |lvl, t| {
            t.hurt_server(lvl, &source, damage);
        });
    }
}

fn apply_impulse(e: &ApplyEntityImpulse, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, entity: EntityId) {
    super::impulse::apply_entity_impulse(e, level, actors, enchantment_level, entity);
}

fn play_sound(e: &PlaySoundEffect, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, entity: EntityId, position: Vec3) {
    if actors.get(level, entity).is_none_or(|t| t.base().is_silent()) {
        return;
    }
    let (volume, pitch) = actors.with_random(level, entity, |r| (e.volume.sample(r), e.pitch.sample(r)));
    let index = mth::clamp_i32(enchantment_level - 1, 0, e.sound_events.len() as i32 - 1);
    let Some(sound) = e.sound_events.get(index as usize).and_then(crate::world::item::holder_sound) else { return };
    let source = if actors.get(level, entity).is_some_and(|t| t.is_player()) {
        mc_protocol::packets::play::SoundSource::Players
    } else {
        mc_protocol::packets::play::SoundSource::Neutral
    };
    level.play_sound_at(None, position.x, position.y, position.z, sound, source, volume, pitch);
}

fn game_event_at(level: &mut ServerLevel, entity: EntityId, event: GameEvent, pos: BlockPos) {
    level.game_event_at_block(event, pos, &GameEventContext::of_entity(Some(entity)));
}

fn replace_block(e: &ReplaceBlock, level: &mut ServerLevel, actors: &mut Actors<'_>, entity: EntityId, position: Vec3) {
    let pos = BlockPos::containing_position(&position).offset(e.offset.x, e.offset.y, e.offset.z);
    if let Some(p) = &e.predicate {
        if !p.predicate.test(&*level, pos) {
            return;
        }
    }
    let Some(provider) = provider_of(level, &e.block_state) else { return };
    let Some(state) = actors.with(level, entity, |lvl, t| provider.get_state(&*lvl, &mut t.base_mut().random, pos)) else { return };
    if level.set_block_and_update(pos, state) {
        if let Some(event) = e.trigger_game_event {
            game_event_at(level, entity, event, pos);
        }
    }
}

fn replace_disk(e: &ReplaceDisk, level: &mut ServerLevel, actors: &mut Actors<'_>, enchantment_level: i32, entity: EntityId, position: Vec3) {
    let center = BlockPos::containing_position(&position).offset(e.offset.x, e.offset.y, e.offset.z);
    let dist = e.radius.calculate(enchantment_level) as i32;
    let height = e.height.calculate(enchantment_level) as i32;
    let Some(provider) = provider_of(level, &e.block_state) else { return };
    let from = center.offset(-dist, 0, -dist);
    let to = center.offset(dist, (height - 1).min(0), dist);
    for pos in BlockPos::between_closed(&from, &to) {
        if pos.dist_to_center_sqr(&Vec3::new(position.x, pos.y() as f64 + 0.5, position.z)) < f64::from(dist.wrapping_mul(dist))
            && e.predicate.as_ref().is_none_or(|p| p.predicate.test(&*level, pos))
        {
            let Some(state) = actors.with(level, entity, |lvl, t| provider.get_state(&*lvl, &mut t.base_mut().random, pos)) else { return };
            if level.set_block_and_update(pos, state) {
                if let Some(event) = e.trigger_game_event {
                    game_event_at(level, entity, event, pos);
                }
            }
        }
    }
}

/// The provider behind `Holder<BlockStateProvider>`: inline, or a registry entry decoded on demand.
fn provider_of(level: &ServerLevel, holder: &Holder<crate::world::item::enchantment::value_codecs::BlockStateProviderRef>) -> Option<std::sync::Arc<dyn crate::world::level::levelgen::stateproviders::BlockStateProvider>> {
    match holder {
        Holder::Direct(d) => Some(d.provider.clone()),
        Holder::Reference(id) => crate::world::item::enchantment::value_codecs::resolve_block_state_provider(&level.registries, id),
    }
}

fn set_block_properties(e: &SetBlockProperties, level: &mut ServerLevel, actors: &mut Actors<'_>, entity: EntityId, position: Vec3) {
    let _ = actors;
    let pos = BlockPos::containing_position(&position).offset(e.offset.x, e.offset.y, e.offset.z);
    let state = level.get_block_state(pos);
    let modified = apply_block_item_state_properties(&e.properties, state);
    if state != modified && level.set_block_and_update(pos, modified) {
        if let Some(event) = e.trigger_game_event {
            game_event_at(level, entity, event, pos);
        }
    }
}

/// `BlockItemStateProperties.apply(state)`: known properties with valid values are set, the rest is ignored.
pub fn apply_block_item_state_properties(properties: &mc_protocol::components::BlockItemStateProperties, state: BlockState) -> BlockState {
    let mut state = state;
    for (name, value) in &properties.properties {
        if let Some(next) = state.with_property_str(name, value) {
            state = next;
        }
    }
    state
}
