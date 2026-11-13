//! `net.minecraft.world.entity.npc.InventoryCarrier`: mobs with a `SimpleContainer` inventory that they fill by picking
//! up items (villagers, allays, piglins).

use crate::server::level::ServerLevel;
use crate::world::container::{Container, SimpleContainer};
use crate::world::entity::Entity;
use crate::world::entity::EntityId;
use crate::world::entity::item::ItemEntity;
use crate::world::entity::mob::ext::take;
use crate::world::item::{ItemStack, ItemStackExt};
use mc_nbt::{CompoundTag, ListTag, Tag};

/// `TAG_INVENTORY`.
pub const TAG_INVENTORY: &str = "Inventory";

/// `InventoryCarrier`.
pub trait InventoryCarrier: Entity + Sized {
    /// `getInventory()`.
    fn get_inventory(&self) -> &SimpleContainer;
    fn get_inventory_mut(&mut self) -> &mut SimpleContainer;
}

/// Slot id of the first inventory slot in `getSlot(slot)` of `Piglin`, `Pillager`, `AbstractVillager` (`SlotRanges.MOB_INVENTORY`).
const INVENTORY_SLOT_OFFSET: i32 = 300;

fn inventory_slot_index<M: InventoryCarrier>(mob: &M, slot: i32) -> Option<usize> {
    let index = usize::try_from(slot - INVENTORY_SLOT_OFFSET).ok()?;
    (index < mob.get_inventory().get_container_size()).then_some(index)
}

/// `getSlot(slot).get()` of the mobs that expose their inventory as `inventory.N`; the other slots are `LivingEntity`'s.
pub fn get_slot<M: InventoryCarrier>(mob: &M, slot: i32) -> Option<ItemStack> {
    match inventory_slot_index(mob, slot) {
        Some(i) => Some(mob.get_inventory().get_item(i).clone()),
        None => crate::advancements::predicates::entity::support::get_slot(mob.as_dyn(), slot),
    }
}

/// `getSlot(slot).set(stack)` of the mobs that expose their inventory as `inventory.N`.
pub fn set_slot<M: InventoryCarrier>(
    mob: &mut M,
    level: &mut ServerLevel,
    slot: i32,
    stack: ItemStack,
) {
    match inventory_slot_index(mob, slot) {
        Some(i) => mob.get_inventory_mut().set_item(i, stack),
        None => {
            crate::advancements::predicates::entity::support::set_slot(
                level,
                mob.as_dyn_mut(),
                slot,
                stack,
            );
        }
    }
}

/// `InventoryCarrier.pickUpItem(level, mob, inventoryCarrier, itemEntity)`.
pub fn pick_up_item<M: InventoryCarrier>(level: &mut ServerLevel, mob: &mut M, item_id: EntityId) {
    let Some(item_stack) = level
        .get_entity(item_id)
        .and_then(|e| e.as_any().downcast_ref::<ItemEntity>())
        .map(|e| e.get_item())
    else {
        return;
    };
    if mob.wants_to_pick_up(level, &item_stack) {
        let has_space = mob.get_inventory().can_add_item(&item_stack);
        if !has_space {
            return;
        }
        crate::world::entity::player::pickup::on_item_pickup_by_id(level, &*mob, item_id);
        let count = item_stack.count();
        let remainder = mob.get_inventory_mut().add_item(&item_stack);
        take(level, &*mob, item_id, count - remainder.count());
        level.with_entity(item_id, |level, item| {
            if let Some(ie) = item.as_any_mut().downcast_mut::<ItemEntity>() {
                if remainder.is_empty() {
                    ie.discard_in_level(level);
                } else {
                    // `itemStack.setCount(remainder.getCount())`
                    let mut stack = ie.get_item();
                    stack.set_count(remainder.count());
                    ie.set_item_in_place(stack);
                }
            }
        });
    }
}

/// `readInventoryFromTag(input)`: `input.list("Inventory", ItemStack.CODEC)` into `fromItemList`.
pub fn read_inventory_from_tag(inventory: &mut SimpleContainer, input: &CompoundTag) {
    if let Some(list) = input.get_list(TAG_INVENTORY) {
        let stacks: Vec<ItemStack> = list
            .compounds()
            .filter_map(crate::world::item::load_item_stack)
            .collect();
        inventory.from_item_list(&stacks);
    }
}

/// `writeInventoryToTag(output)`: `getInventory().storeAsItemList(output.list("Inventory", ItemStack.CODEC))`.
pub fn write_inventory_to_tag(inventory: &SimpleContainer, out: &mut CompoundTag) {
    let mut list = ListTag::new();
    for stack in inventory.store_as_item_list() {
        let mut tag = CompoundTag::new();
        stack.save_into(&mut tag);
        list.push(Tag::Compound(tag));
    }
    out.put(TAG_INVENTORY, Tag::List(list));
}
