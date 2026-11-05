//! The data-component driven base of `Item.use` / `Item.finishUsingItem`: `consumable` (+ `food`, consume effects),
//! swappable `equippable`, `blocks_attacks`, `kinetic_weapon`.
//!
//! The mob-effect parts (potion contents / stew / ominous bottle listeners, status-effect consume effects) are
//! `world::effect::consume`.

use super::{ConsumableExt, InteractionResult, ItemStack, ItemStackExt};
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::InteractionHand;
use mc_core::random::RandomSource;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::components::{
    CONSUMABLE, ConsumeEffect, Consumable, EQUIPPABLE, FOOD, Holder, ItemUseAnimation, KINETIC_WEAPON, SoundEvent as ComponentSoundEvent,
};
use mc_protocol::packets::play::SoundSource;
use crate::world::effect as effects;

/// `Holder<SoundEvent>` -> registered sound (inline `Direct` sound events are not expressible by `SoundEvent`).
pub fn holder_sound(holder: &Holder<ComponentSoundEvent>) -> Option<SoundEvent> {
    match holder {
        Holder::Reference(n) => SoundEvent::by_name(&n.to_string()),
        Holder::Direct(d) => SoundEvent::by_name(&d.location.to_string()),
    }
}

/// `Entity.getSoundSource()`.
fn sound_source(entity: &dyn Entity) -> SoundSource {
    if entity.is_player() { SoundSource::Players } else { SoundSource::Neutral }
}

/// `Entity.playSound(sound, volume, pitch)`.
pub fn entity_play_sound(level: &mut ServerLevel, entity: &dyn Entity, sound: SoundEvent, volume: f32, pitch: f32) {
    if !entity.base().is_silent() {
        let p = entity.position();
        level.play_sound_at(None, p.x, p.y, p.z, sound, sound_source(entity), volume, pitch);
    }
}

/// `Consumable.emitParticlesAndSounds(random, user, stack, particleCount)`; the particle spawn is client-side in
/// vanilla but still consumes four random floats per particle from the user's random.
pub fn emit_particles_and_sounds(consumable: &Consumable, level: &mut ServerLevel, user: &mut dyn Entity, stack: &ItemStack, particle_count: i32) {
    let random = &mut user.base_mut().random;
    let eat_volume = if random.next_boolean() { 0.5f32 } else { 1.0f32 };
    let eat_pitch = random.triangle_f32(1.0, 0.2);
    let drink_pitch = mc_core::math::mth::random_between(random, 0.9, 1.0);
    let volume = if consumable.animation == ItemUseAnimation::Drink { 0.5 } else { eat_volume };
    let pitch = if consumable.animation == ItemUseAnimation::Drink { drink_pitch } else { eat_pitch };
    if consumable.has_consume_particles && !stack.is_empty() {
        for _ in 0..particle_count {
            // `spawnItemParticles`: (nextFloat - 0.5) * 0.1, nextFloat * 0.1 + 0.1, nextFloat * 0.6, (nextFloat - 0.5) * 0.3
            for _ in 0..4 {
                random.next_float();
            }
        }
    }
    if let Some(sound) = holder_sound(&consumable.sound) {
        entity_play_sound(level, user, sound, volume, pitch);
    }
}

/// `ConsumeEffect.apply(level, stack, user)`.
pub fn apply_consume_effect(effect: &ConsumeEffect, level: &mut ServerLevel, user: &mut dyn Entity) {
    match effect {
        ConsumeEffect::PlaySound(p) => {
            if let Some(sound) = holder_sound(&p.sound) {
                // `level.playSound(null, user.blockPosition(), sound, user.getSoundSource(), 1.0F, 1.0F)`
                let pos = user.block_position();
                let source = sound_source(user);
                level.play_sound(None, pos, sound, source, 1.0, 1.0);
            }
        }
        ConsumeEffect::ApplyStatusEffects(a) => {
            effects::consume::apply_status_effects(a, level, user);
        }
        ConsumeEffect::RemoveStatusEffects(r) => {
            effects::consume::remove_status_effects(r, level, user);
        }
        ConsumeEffect::ClearAllStatusEffects => {
            user.remove_all_effects(level);
        }
        ConsumeEffect::TeleportRandomly(t) => {
            crate::world::entity::living_entity::teleport::apply_teleport_randomly(t, level, user);
        }
    }
}

/// `Consumable.onConsume(level, user, stack)`; returns the (shrunk) stack.
pub fn on_consume(consumable: &Consumable, level: &mut ServerLevel, user: &mut dyn Entity, stack: &mut ItemStack) -> ItemStack {
    emit_particles_and_sounds(consumable, level, user, stack, 16);
    // `serverPlayer.awardStat(ITEM_USED)` / `CriteriaTriggers.CONSUME_ITEM`: statistics/advancements are later waves.
    // ConsumableListeners: `FoodProperties.onConsume`, then the effect-applying ones (potion contents, suspicious
    // stew, ominous bottle)
    if let Some(food) = stack.get(&FOOD).cloned() {
        let sound = holder_sound(&consumable.sound);
        let (x, y, z) = {
            let p = user.position();
            (p.x, p.y, p.z)
        };
        let pitch = user.base_mut().random.triangle_f32(1.0, 0.4);
        if let Some(s) = sound {
            level.play_sound_at(None, x, y, z, s, SoundSource::Neutral, 1.0, pitch);
        }
        if let Some(player) = user.player_mut() {
            player.food_data.eat_properties(food.nutrition, food.saturation);
            let burp_pitch = mc_core::math::mth::random_between(&mut user.base_mut().random, 0.9, 1.0);
            level.play_sound_at(None, x, y, z, SoundEvent::ENTITY_PLAYER_BURP, SoundSource::Players, 0.5, burp_pitch);
        }
    }
    effects::consume::on_consume_listeners(level, user, stack);
    for effect in &consumable.on_consume_effects {
        apply_consume_effect(effect, level, user);
    }
    let event = if consumable.animation == ItemUseAnimation::Drink { GameEvent::DRINK } else { GameEvent::EAT };
    level.game_event_from_entity(Some(user.id()), event, user.position());
    let infinite = user.player().is_some_and(|p| p.abilities.instabuild);
    stack.consume(1, infinite);
    stack.clone()
}

/// `Consumable.canConsume(user, stack)`.
fn can_consume(user: &dyn Entity, stack: &ItemStack) -> bool {
    match (stack.get(&FOOD), user.player()) {
        (Some(food), Some(p)) => p.abilities.invulnerable || food.can_always_eat || p.food_data.needs_food(),
        _ => true,
    }
}

/// `Item.finishUsingItem(stack, level, entity)`.
pub fn base_finish_using_item(stack: &mut ItemStack, level: &mut ServerLevel, entity: &mut dyn Entity) -> ItemStack {
    match stack.get(&CONSUMABLE).cloned() {
        Some(c) => on_consume(&c, level, entity, stack),
        None => stack.clone(),
    }
}

/// `Item.use(level, player, hand)`.
pub fn base_use(level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
    let stack = player.get_item_in_hand(hand);
    if let Some(consumable) = stack.get(&CONSUMABLE).cloned() {
        // `Consumable.startConsuming`
        if !can_consume(player, &stack) {
            return InteractionResult::FAIL;
        }
        if consumable.consume_ticks() > 0 {
            player.start_using_item(level, hand);
            return InteractionResult::CONSUME;
        }
        let mut s = stack;
        let result = on_consume(&consumable, level, player, &mut s);
        return match InteractionResult::CONSUME {
            InteractionResult::Success(success) => InteractionResult::Success(success.held_item_transformed_to(result)),
            other => other,
        };
    }
    if let Some(equippable) = stack.get(&EQUIPPABLE).cloned() {
        if equippable.swappable {
            return swap_with_equipment_slot(&equippable, level, player, hand);
        }
    }
    if stack.has(mc_data::DataComponentType::BLOCKS_ATTACKS) {
        player.start_using_item(level, hand);
        return InteractionResult::CONSUME;
    }
    if let Some(kinetic) = stack.get(&KINETIC_WEAPON) {
        let sound = kinetic.sound.as_ref().and_then(holder_sound);
        player.start_using_item(level, hand);
        if let Some(sound) = sound {
            // `makeSound`: `level.playSound(causer, ...)` — excludes the causer
            let p = player.position();
            level.play_sound_at(Some(player.id()), p.x, p.y, p.z, sound, SoundSource::Players, 1.0, 1.0);
        }
        return InteractionResult::CONSUME;
    }
    InteractionResult::PASS
}

/// `Equippable.swapWithEquipmentSlot(inHand, player)`.
fn swap_with_equipment_slot(equippable: &mc_protocol::components::Equippable, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
    let slot = EquipmentSlot::from_protocol(equippable.slot);
    let mut in_hand = player.get_item_in_hand(hand);
    let can_be_equipped = equippable.allowed_entities.as_ref().is_none_or(|s| s.contains(mc_data::EntityType::PLAYER));
    if !(can_use_slot(player, slot) && can_be_equipped) {
        return InteractionResult::PASS;
    }
    let in_slot = player.player.inventory.equipment_get(slot).clone();
    let creative = player.is_creative();
    if (super::enchantment_helper::has(&level.registries, &in_slot, super::enchantment::enchantment_effect_components::EffectComponent::prevent_armor_change) && !creative) || ItemStack::is_same_item_same_components(&in_hand, &in_slot) {
        return InteractionResult::FAIL;
    }
    // `player.awardStat(ITEM_USED)`
    let infinite = player.player.abilities.instabuild;
    if in_hand.count() <= 1 {
        // Java aliases `swappedToHand` to the hand stack object when the slot is empty; `copyAndClear` below then
        // empties it (survival), so the hand becomes empty. In creative the hand copy is left untouched.
        let swapped_to_hand = if in_slot.is_empty() {
            if creative { in_hand.clone() } else { ItemStack::empty() }
        } else {
            let mut s = in_slot;
            s.copy_and_clear()
        };
        let swapped_to_equipment = if creative { in_hand.copy() } else { in_hand.copy_and_clear() };
        if !creative {
            player.set_item_in_hand(hand, ItemStack::empty());
        }
        player.set_item_slot(level, slot, swapped_to_equipment);
        match InteractionResult::SUCCESS {
            InteractionResult::Success(s) => InteractionResult::Success(s.held_item_transformed_to(swapped_to_hand)),
            o => o,
        }
    } else {
        let mut old = in_slot;
        let swapped_to_inventory = old.copy_and_clear();
        let swapped_to_equipment = in_hand.consume_and_return(1, infinite);
        player.set_item_in_hand(hand, in_hand.clone());
        player.set_item_slot(level, slot, swapped_to_equipment);
        let mut to_inv = swapped_to_inventory;
        if !player.player.inventory.add(&mut to_inv, infinite) {
            player.pending_drops.push((to_inv, false));
        }
        match InteractionResult::SUCCESS {
            InteractionResult::Success(s) => InteractionResult::Success(s.held_item_transformed_to(in_hand)),
            o => o,
        }
    }
}

/// `LivingEntity.canUseSlot(slot)` for players: every slot.
fn can_use_slot(_player: &ServerPlayer, _slot: EquipmentSlot) -> bool {
    true
}
