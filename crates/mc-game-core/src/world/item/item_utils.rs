//! `ItemUtils` (the parts that only need the inventory API).
use super::ItemStack;
use crate::world::inventory::{MenuPlayer, Prediction};

/// `createFilledResult(itemStack, player, newItemStack, limitCreativeStackSize)`.
pub fn create_filled_result(
    item_stack: ItemStack,
    player: &mut dyn MenuPlayer,
    mut new_item_stack: ItemStack,
    limit_creative_stack_size: bool,
) -> ItemStack {
    create_filled_result_in_place(
        item_stack,
        player,
        &mut new_item_stack,
        limit_creative_stack_size,
    )
}

/// `createFilledResult` for a caller that keeps using `newItemStack` afterwards (a trigger fires with it): the inventory add moves
/// the stack out, leaving it empty, exactly as it does for the vanilla object.
pub fn create_filled_result_in_place(
    mut item_stack: ItemStack,
    player: &mut dyn MenuPlayer,
    new_item_stack: &mut ItemStack,
    limit_creative_stack_size: bool,
) -> ItemStack {
    let is_creative = player.has_infinite_materials();
    if limit_creative_stack_size && is_creative {
        if !player.inventory().contains(new_item_stack) {
            player.inventory_mut().add(new_item_stack, true);
        }
        item_stack
    } else {
        // `itemStack.consume(1, player)`: shrink unless the player has infinite materials.
        if !is_creative {
            item_stack.shrink(1);
        }
        if item_stack.is_empty() {
            new_item_stack.clone()
        } else {
            let infinite = player.has_infinite_materials();
            if !player.inventory_mut().add(new_item_stack, infinite) {
                player.drop(new_item_stack.clone(), false, Prediction::Predicted);
            }
            item_stack
        }
    }
}

/// `ItemUtils.onContainerDestroyed(container, contents)`: spawns each stack as an item entity at `pos`.
pub fn on_container_destroyed(
    level: &mut crate::server::level::ServerLevel,
    pos: mc_core::phys::Vec3,
    contents: Vec<ItemStack>,
) {
    for stack in contents {
        let entity =
            crate::world::entity::item::ItemEntity::with_stack(level, pos.x, pos.y, pos.z, stack);
        level.add_fresh_entity(Box::new(entity));
    }
}
