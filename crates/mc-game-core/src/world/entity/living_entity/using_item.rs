//! `LivingEntity`'s using-item state machine for non-player living entities (mobs): `isUsingItem`, `getUsedItemHand`,
//! `startUsingItem`, `updatingUsingItem`, `updateUsingItem`, `completeUsingItem`, `releaseUsingItem`, `stopUsingItem`,
//! `getTicksUsingItem`. `ServerPlayer` has its own copy (`player/item_use.rs`, equipment lives in the inventory there).

use crate::server::level::ServerLevel;
use crate::server::level::level_events::encode_packet;
use crate::world::entity::Entity;
use crate::world::entity::living::{LIVING_ENTITY_FLAG_IS_USING, LIVING_ENTITY_FLAG_OFF_HAND};
use crate::world::item::{InteractionHand, ItemStack, behaviour};
use mc_data::GameEvent;
use mc_protocol::components::USE_EFFECTS;
use mc_protocol::packets::play::ClientboundSetEquipmentPacket;

fn flag(e: &dyn Entity, flag: i32) -> bool {
    e.living().is_some_and(|l| l.get_living_entity_flag(flag))
}

/// `isUsingItem()`.
pub fn is_using_item(e: &dyn Entity) -> bool {
    flag(e, LIVING_ENTITY_FLAG_IS_USING)
}

/// `getUsedItemHand()`.
pub fn get_used_item_hand(e: &dyn Entity) -> InteractionHand {
    if flag(e, LIVING_ENTITY_FLAG_OFF_HAND) {
        InteractionHand::OffHand
    } else {
        InteractionHand::MainHand
    }
}

/// `getUseItem()`.
pub fn get_use_item(e: &dyn Entity) -> ItemStack {
    e.living().map(|l| l.use_item.clone()).unwrap_or_default()
}

/// `getUseItemRemainingTicks()`.
pub fn get_use_item_remaining_ticks(e: &dyn Entity) -> i32 {
    e.living().map_or(0, |l| l.use_item_remaining)
}

/// `getTicksUsingItem()`.
pub fn get_ticks_using_item(e: &dyn Entity) -> i32 {
    if is_using_item(e) {
        let use_item = get_use_item(e);
        behaviour(use_item.item()).get_use_duration(&use_item) - get_use_item_remaining_ticks(e)
    } else {
        0
    }
}

fn item_in_hand(e: &dyn Entity, hand: InteractionHand) -> ItemStack {
    e.get_item_by_slot(hand.as_equipment_slot())
}

/// `ItemStack.causeUseVibration(causer, event)`.
fn cause_use_vibration(
    e: &dyn Entity,
    level: &mut ServerLevel,
    stack: &ItemStack,
    event: GameEvent,
) {
    if stack
        .get(&USE_EFFECTS)
        .is_some_and(|u| u.interact_vibrations)
    {
        level.game_event_from_entity_ref(e, event, e.base().position());
    }
}

/// `startUsingItem(hand)`.
pub fn start_using_item(e: &mut dyn Entity, level: &mut ServerLevel, hand: InteractionHand) {
    let stack = item_in_hand(e, hand);
    if stack.is_empty() || is_using_item(e) {
        return;
    }
    {
        let living = e.living_mut().expect("living");
        living.use_item = stack.clone();
        living.use_item_remaining = behaviour(stack.item()).get_use_duration(&stack);
    }
    // `updatePlayersWithNewEquipment`
    let packet = ClientboundSetEquipmentPacket {
        entity: e.id(),
        slots: vec![(hand.as_equipment_slot().to_protocol(), stack.clone())],
    };
    level
        .entity_tracker
        .send_to_tracking_players(e.id(), &encode_packet(&packet));
    {
        let living = e.living_mut().expect("living");
        living.set_living_entity_flag(LIVING_ENTITY_FLAG_IS_USING, true);
        living.set_living_entity_flag(
            LIVING_ENTITY_FLAG_OFF_HAND,
            hand == InteractionHand::OffHand,
        );
    }
    cause_use_vibration(e, level, &stack, GameEvent::ITEM_INTERACT_START);
    // `recentKineticEnemies = new Object2LongOpenHashMap()` for a kinetic weapon: owned by the combat code.
}

/// `stopUsingItem()`.
pub fn stop_using_item(e: &mut dyn Entity, level: &mut ServerLevel) {
    let was_using = is_using_item(e);
    e.living_mut()
        .expect("living")
        .set_living_entity_flag(LIVING_ENTITY_FLAG_IS_USING, false);
    if was_using {
        let stack = get_use_item(e);
        cause_use_vibration(e, level, &stack, GameEvent::ITEM_INTERACT_FINISH);
    }
    let living = e.living_mut().expect("living");
    living.use_item = ItemStack::empty();
    living.use_item_remaining = 0;
}

/// `releaseUsingItem()`.
pub fn release_using_item(e: &mut dyn Entity, level: &mut ServerLevel) {
    let hand = get_used_item_hand(e);
    let mut in_hand = item_in_hand(e, hand);
    let use_item = get_use_item(e);
    if !use_item.is_empty() && ItemStack::is_same_item(&in_hand, &use_item) {
        e.living_mut().expect("living").use_item = in_hand.clone();
        let remaining = get_use_item_remaining_ticks(e);
        let before = in_hand.copy();
        let released = behaviour(in_hand.item()).release_using(&mut in_hand, level, e, remaining);
        // the hook mutates the stack the entity holds (`this.useItem` aliases the hand stack)
        let stored = if released {
            crate::world::item::apply_after_use_component_side_effects(
                level,
                in_hand.clone(),
                e,
                &before,
            )
        } else {
            in_hand.clone()
        };
        if stored != item_in_hand(e, hand) {
            e.set_item_slot(level, hand.as_equipment_slot(), stored);
        }
        let use_item = item_in_hand(e, hand);
        e.living_mut().expect("living").use_item = use_item.clone();
        if behaviour(use_item.item()).use_on_release(&use_item) {
            updating_using_item(e, level);
        }
    }
    stop_using_item(e, level);
}

/// `updatingUsingItem()` (`LivingEntity.tick`).
pub fn updating_using_item(e: &mut dyn Entity, level: &mut ServerLevel) {
    if is_using_item(e) {
        let hand = get_used_item_hand(e);
        let in_hand = item_in_hand(e, hand);
        if ItemStack::is_same_item(&in_hand, &get_use_item(e)) {
            e.living_mut().expect("living").use_item = in_hand;
            update_using_item(e, level);
        } else {
            stop_using_item(e, level);
        }
    }
}

/// `updateUsingItem(useItem)`.
fn update_using_item(e: &mut dyn Entity, level: &mut ServerLevel) {
    let mut stack = get_use_item(e);
    let remaining = get_use_item_remaining_ticks(e);
    crate::world::item::on_use_tick(&mut stack, level, e, remaining);
    // the hook may have changed the stack (charge): vanilla mutates the shared object
    let hand = get_used_item_hand(e);
    if stack != item_in_hand(e, hand) {
        e.set_item_slot(level, hand.as_equipment_slot(), stack.clone());
    }
    let using = is_using_item(e);
    let living = e.living_mut().expect("living");
    // the hook may have released the item (`stopUsingItem` cleared `useItem`)
    if using {
        living.use_item = stack.clone();
    }
    living.use_item_remaining -= 1;
    if living.use_item_remaining == 0 && !behaviour(stack.item()).use_on_release(&stack) {
        complete_using_item(e, level);
    }
}

/// `completeUsingItem()`.
pub fn complete_using_item(e: &mut dyn Entity, level: &mut ServerLevel) {
    let hand = get_used_item_hand(e);
    let use_item = get_use_item(e);
    if use_item != item_in_hand(e, hand) {
        release_using_item(e, level);
    } else if !use_item.is_empty() && is_using_item(e) {
        let mut stack = use_item;
        let result = crate::world::item::finish_using_item(&mut stack, level, e);
        if result != item_in_hand(e, hand) || stack != item_in_hand(e, hand) {
            e.set_item_slot(level, hand.as_equipment_slot(), result);
        }
        stop_using_item(e, level);
    }
}
