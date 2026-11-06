//! `Entity.interact` (leash handling), `Mob.interact` / `checkAndHandleImportantInteractions` / `mobInteract`,
//! `usePlayerItem`.

use super::ext;
use super::leashable::{self, LeashHolder};
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{InteractionHand, InteractionResult};
use crate::world::item::{ItemStack, hurt_and_break};
use mc_core::phys::Vec3;
use mc_data::{GameEvent, Item, SoundEvent};
use mc_protocol::components::USE_REMAINDER;

/// Runs `f` with the stack in `hand` of `player` (the Java code aliases the hand stack; the result is written back).
pub fn with_hand_stack<R>(
    player: &mut ServerPlayer,
    hand: InteractionHand,
    f: impl FnOnce(&mut ItemStack, &mut ServerPlayer) -> R,
) -> R {
    let slot = hand.as_equipment_slot();
    let mut stack = player.get_item_in_hand(hand);
    let result = f(&mut stack, player);
    if let Some(s) = player.item_by_slot_mut(slot) {
        *s = stack;
    }
    result
}

/// `Mob.usePlayerItem(player, hand, itemStack)`: consumes one item and applies the `use_remainder`.
pub fn use_player_item(level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) {
    let infinite = player.player.abilities.instabuild;
    let slot = hand.as_equipment_slot();
    let before = player.get_item_in_hand(hand);
    let before_use_count = before.count();
    let remainder = before.get(&USE_REMAINDER).cloned();
    let mut stack = before;
    if !infinite {
        stack.shrink(1);
    }
    if let Some(r) = remainder {
        // `UseRemainder.convertIntoRemainder(stack, countBefore, hasInfiniteMaterials, handleExtraItemsCreatedOnUse)`
        let new_hand_stack = if infinite || stack.count() >= before_use_count {
            stack
        } else {
            let remainder_stack = r.convert_into.create();
            if stack.is_empty() {
                remainder_stack
            } else {
                let mut extra = remainder_stack;
                if !player.player.inventory.add(&mut extra, infinite) {
                    player.drop(
                        level,
                        extra,
                        false,
                        crate::world::inventory::Prediction::Predicted,
                    );
                }
                stack
            }
        };
        player.set_item_in_hand_slot(level, hand, new_hand_stack);
    } else if let Some(s) = player.item_by_slot_mut(slot) {
        *s = stack;
    }
}

/// `Entity.dropAllLeashConnections(player)`.
pub fn drop_all_leash_connections(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: Option<crate::world::entity::EntityId>,
) -> bool {
    let leashed_to_me = leashable::leashable_leashed_to(level, e);
    let mut dropped = !leashed_to_me.is_empty();
    if leashable::is_leashed(e) {
        leashable::drop_leash(e, level);
        dropped = true;
    }
    for id in leashed_to_me {
        level.with_entity(id, |level, l| leashable::drop_leash(l, level));
    }
    if dropped {
        level.game_event_from_entity(player, GameEvent::SHEAR, e.position());
    }
    dropped
}

/// `Entity.shearOffAllLeashConnections(player)`.
pub fn shear_off_all_leash_connections(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: Option<&ServerPlayer>,
) -> bool {
    let dropped = drop_all_leash_connections(e, level, player.map(|p| p.id()));
    if dropped {
        let source = player.map_or_else(|| e.get_sound_source(), |p| p.get_sound_source());
        level.play_sound(
            None,
            e.block_position(),
            SoundEvent::ITEM_SHEARS_SNIP,
            source,
            1.0,
            1.0,
        );
    }
    dropped
}

/// `Entity.interact(player, hand, location)` for mobs (the `Leashable` parts and shearing of equipment).
pub fn entity_interact(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    if player.is_shift_key_down() && e.can_be_leashed_in(level) && e.is_alive() && !e.is_baby() {
        let player_id = player.id();
        let mobs_to_leash =
            leashable::leashable_in_area(level, e.base().bounding_box().get_center(), |l| {
                leashable::get_leash_holder(l) == Some(player_id)
            });
        if !mobs_to_leash.is_empty() {
            let mut any_leashed = false;
            let holder = LeashHolder::of(e);
            for id in mobs_to_leash {
                let can = level
                    .get_entity(id)
                    .is_some_and(|m| leashable::can_have_a_leash_attached_to(level, m, e.as_dyn()));
                if can {
                    level.with_entity(id, |level, m| {
                        leashable::set_leashed_to(m, level, holder, true)
                    });
                    any_leashed = true;
                }
            }
            if any_leashed {
                level.game_event_from_entity(
                    Some(player.id()),
                    GameEvent::ENTITY_ACTION,
                    e.position(),
                );
                e.play_sound_default(level, SoundEvent::ITEM_LEAD_TIED);
                return match InteractionResult::SUCCESS_SERVER {
                    InteractionResult::Success(s) => InteractionResult::Success(s.without_item()),
                    other => other,
                };
            }
        }
    }
    let held = player.get_item_in_hand(hand);
    if held.is(Item::SHEARS) && shear_off_all_leash_connections(e, level, Some(player)) {
        with_hand_stack(player, hand, |s, p| {
            hurt_and_break(s, 1, level, p, hand.as_equipment_slot())
        });
        return InteractionResult::SUCCESS;
    }
    if held.is(Item::SHEARS)
        && e.can_shear_equipment(player)
        && !player.is_shift_key_down()
        && attempt_to_shear_equipment(e, level, player, hand)
    {
        return InteractionResult::SUCCESS;
    }
    if e.is_alive() && e.mob().is_some() {
        if leashable::get_leash_holder(e) == Some(player.id()) {
            if player.player.abilities.instabuild {
                leashable::remove_leash(e, level);
            } else {
                leashable::drop_leash(e, level);
            }
            e.game_event(level, GameEvent::ENTITY_INTERACT);
            e.play_sound_default(level, SoundEvent::ITEM_LEAD_UNTIED);
            return match InteractionResult::SUCCESS {
                InteractionResult::Success(s) => InteractionResult::Success(s.without_item()),
                other => other,
            };
        }
        let item_stack = player.get_item_in_hand(hand);
        let holder_is_player = leashable::get_leash_holder(e)
            .and_then(|h| level.get_entity(h))
            .is_some_and(|h| h.is_player());
        if item_stack.is(Item::LEAD)
            && !holder_is_player
            && leashable::can_have_a_leash_attached_to(level, e, &*player)
        {
            if leashable::is_leashed(e) {
                leashable::drop_leash(e, level);
            }
            leashable::set_leashed_to(e, level, LeashHolder::of(&*player), true);
            e.play_sound_default(level, SoundEvent::ITEM_LEAD_TIED);
            with_hand_stack(player, hand, |s, _| s.shrink(1));
            return InteractionResult::SUCCESS_SERVER;
        }
    }
    InteractionResult::PASS
}

/// `Mob.attemptToShearEquipment(player, hand, heldItem)`.
pub fn attempt_to_shear_equipment(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> bool {
    use crate::world::item::enchantment::enchantment_effect_components::EffectComponent;
    use mc_protocol::components::EQUIPPABLE;
    for slot in EquipmentSlot::VALUES {
        let item_stack = e.get_item_by_slot(slot);
        let Some(eq) = item_stack.get(&EQUIPPABLE) else {
            continue;
        };
        if eq.can_be_sheared
            && (!crate::world::item::enchantment_helper::has(
                &level.registries,
                &item_stack,
                EffectComponent::prevent_armor_change,
            ) || player.player.abilities.instabuild)
        {
            let shearing_sound = eq
                .shearing_sound
                .name()
                .and_then(|n| SoundEvent::by_name(&n.to_string()));
            // shearItem
            with_hand_stack(player, hand, |s, p| {
                hurt_and_break(s, 1, level, p, hand.as_equipment_slot())
            });
            let offset = e
                .get_attachments()
                .get_average(crate::world::entity::EntityAttachment::Passenger);
            super::equipment::set_item_slot(e, level, slot, ItemStack::empty());
            level.game_event_from_entity(Some(player.id()), GameEvent::SHEAR, e.position());
            crate::world::entity::item::spawn_at_location(level, e, item_stack.clone(), offset);
            ext::trigger_player_sheared_equipment(level, player.id(), &item_stack, &*e);
            if let Some(sound) = shearing_sound {
                e.play_sound_default(level, sound);
            }
            return true;
        }
    }
    false
}

/// `Mob.checkAndHandleImportantInteractions(player, hand)`: name tags and spawn eggs.
fn check_and_handle_important_interactions(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    let item_stack = player.get_item_in_hand(hand);
    if item_stack.is(Item::NAME_TAG) {
        let result = with_hand_stack(player, hand, |s, p| {
            crate::world::item::interact_living_entity(level, s, p, e, hand)
        });
        if result.consumes_action() {
            return result;
        }
    }
    match with_hand_stack(player, hand, |s, p| {
        ext::spawn_offspring_from_spawn_egg(level, p, e, s)
    }) {
        None => InteractionResult::PASS,
        Some(None) => InteractionResult::PASS,
        Some(Some(offspring)) => {
            e.on_offspring_spawned_from_egg(level, player, offspring);
            InteractionResult::SUCCESS_SERVER
        }
    }
}

/// `Mob.interact(player, hand, location)`.
pub fn interact(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
    _location: Vec3,
) -> InteractionResult {
    if !e.is_alive() {
        return InteractionResult::PASS;
    }
    let result = check_and_handle_important_interactions(e, level, player, hand);
    if result.consumes_action() {
        e.game_event(level, GameEvent::ENTITY_INTERACT);
        return result;
    }
    let super_reaction = entity_interact(e, level, player, hand);
    if super_reaction != InteractionResult::PASS {
        return super_reaction;
    }
    let result = e.mob_interact(level, player, hand);
    if result.consumes_action() {
        e.game_event(level, GameEvent::ENTITY_INTERACT);
        return result;
    }
    InteractionResult::PASS
}

/// `Mob.mobInteract` of the generic layers: `Animal` feeding, then `AgeableMob`'s golden dandelion, then `Mob` (PASS). The
/// `super.mobInteract` of every concrete class.
pub fn mob_interact(
    e: &mut dyn Entity,
    level: &mut ServerLevel,
    player: &mut ServerPlayer,
    hand: InteractionHand,
) -> InteractionResult {
    if e.mob().is_some_and(|m| m.animal.is_some())
        && let Some(r) = super::animal::mob_interact(e, level, player, hand)
    {
        return r;
    }
    if e.mob().is_some_and(|m| m.ageable.is_some()) {
        let creative = player.player.abilities.instabuild;
        let r = with_hand_stack(player, hand, |stack, _| {
            super::ageable::golden_dandelion_interact(e, level, stack, creative)
        });
        if let Some(r) = r {
            return r;
        }
    }
    InteractionResult::PASS
}
