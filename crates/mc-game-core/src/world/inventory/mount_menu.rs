//! `AbstractMountInventoryMenu`, `HorseInventoryMenu`, `NautilusInventoryMenu`: the saddle / body armor / chest screen of
//! rideable mobs. The mob supplies its rules through [`MountInventory`] (`Entity::as_mount_inventory`) and its chest
//! container through `Entity::as_container`; open it with `ServerPlayer::open_mount_inventory`.

use super::{
    AbstractContainerMenu, ContainerKey, MenuBase, MenuPlayer, Slot, SlotBehavior, SlotEnv,
};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::{Entity, EntityId};
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter as _;
use std::any::Any;

/// What the mount menus ask of the mob (`AbstractHorse` / `AbstractNautilus` / `Llama`).
pub trait MountInventory {
    /// `getInventoryColumns()`.
    fn get_inventory_columns(&self) -> i32;
    /// `hasInventoryChanged(container)` = the mob replaced its inventory container since the menu was built: bump a
    /// counter whenever that happens (`createInventory`).
    fn inventory_epoch(&self) -> u32 {
        0
    }
    /// `canUseSlot(slot)`.
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool;
    /// `isEquippableInSlot(stack, slot)`.
    fn is_equippable_in_slot(&self, stack: &ItemStack, slot: EquipmentSlot) -> bool;
    /// `getItemBySlot(slot)`.
    fn get_equipment_slot_item(&self, slot: EquipmentSlot) -> ItemStack;
    /// The `setTheItem` of `Mob.createEquipmentSlotContainer(slot)`: `setItemSlot(slot, stack)` and, for a non-empty
    /// stack, `setGuaranteedDrop(slot)` + `setPersistenceRequired()`.
    fn set_equipment_slot_item(&mut self, slot: EquipmentSlot, stack: ItemStack);
    /// `instanceof Llama` (only selects the armor slot sprite on the client; kept for completeness).
    fn is_llama(&self) -> bool {
        false
    }
}

/// `ArmorSlot` whose owner is the mount: the equipment slot of that entity.
struct MountArmorSlot {
    mount: EntityId,
    slot: EquipmentSlot,
}

impl SlotBehavior for MountArmorSlot {
    fn set_by_player(
        &self,
        slot: &Slot,
        env: &mut SlotEnv,
        stack: ItemStack,
        previous: &ItemStack,
    ) {
        let (mount, equipment) = (self.mount, self.slot);
        if let Some(level) = env.player.level() {
            level.with_entity(mount, |level, e| {
                e.on_equip_item(level, equipment, previous, &stack)
            });
        }
        slot.set(env, stack);
    }
    fn get_max_stack_size(&self, _slot: &Slot, _env: &mut SlotEnv) -> i32 {
        1
    }
    fn may_place(&self, _slot: &Slot, env: &mut SlotEnv, stack: &ItemStack) -> bool {
        let (mount, equipment) = (self.mount, self.slot);
        env.player
            .level_ref()
            .and_then(|l| l.get_entity(mount))
            .and_then(|e| e.as_mount_inventory())
            .is_some_and(|m| m.is_equippable_in_slot(stack, equipment))
    }
    fn may_pickup(&self, slot: &Slot, env: &mut SlotEnv) -> bool {
        let item = slot.get_item(env);
        !(!item.is_empty()
            && !env.player.is_creative()
            && crate::world::item::enchantment_helper::has(&env.player.registries(), &item, crate::world::item::enchantment::enchantment_effect_components::EffectComponent::prevent_armor_change))
    }
}

/// Shared state/behavior of the two mount menus.
struct MountMenuCore {
    base: MenuBase,
    mount: EntityId,
    container: super::ContainerRef,
    epoch: u32,
}

impl MountMenuCore {
    fn new(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        mount: EntityId,
        layout: impl FnOnce(&mut MenuBase, &super::ContainerRef, i32),
    ) -> Self {
        let mut base = MenuBase::new(None, container_id);
        let (size, epoch, columns) = player
            .level_ref()
            .and_then(|l| l.get_entity(mount))
            .map(|e| {
                (
                    e.as_container().map_or(0, |c| c.get_container_size()),
                    e.as_mount_inventory().map_or(0, |m| m.inventory_epoch()),
                    e.as_mount_inventory()
                        .map_or(0, |m| m.get_inventory_columns()),
                )
            })
            .unwrap_or((0, 0, 0));
        let container = super::ContainerRef::entity(mount, size);
        container.start_open(player);
        base.add_slot(Slot::with_behavior(
            ContainerKey::EntityEquipment(mount, EquipmentSlot::Saddle),
            0,
            8,
            18,
            Box::new(MountArmorSlot {
                mount,
                slot: EquipmentSlot::Saddle,
            }),
        ));
        base.add_slot(Slot::with_behavior(
            ContainerKey::EntityEquipment(mount, EquipmentSlot::Body),
            0,
            8,
            36,
            Box::new(MountArmorSlot {
                mount,
                slot: EquipmentSlot::Body,
            }),
        ));
        layout(&mut base, &container, columns);
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self {
            base,
            mount,
            container,
            epoch,
        }
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        let Some(level) = player.level_ref() else {
            return false;
        };
        let Some(entity) = level.get_entity(self.mount) else {
            return false;
        };
        let unchanged = entity
            .as_mount_inventory()
            .is_some_and(|m| m.inventory_epoch() == self.epoch);
        unchanged
            && self.container.still_valid(player)
            && entity.is_alive()
            && player.is_within_entity_interaction_range(&entity.base().bounding_box(), 4.0)
    }

    fn quick_move_stack(
        menu: &mut dyn AbstractContainerMenu,
        player: &mut dyn MenuPlayer,
        slot_index: usize,
        container_size: usize,
    ) -> ItemStack {
        let mut clicked = ItemStack::empty();
        if !menu
            .base_mut()
            .with_slot(player, slot_index, |s, e| s.has_item(e))
        {
            return clicked;
        }
        let mut stack = menu
            .base_mut()
            .with_slot(player, slot_index, |s, e| s.get_item(e));
        clicked = stack.copy();
        let player_container_start = 2 + container_size;
        let total = menu.base().slots.len();
        let slot1_free = menu
            .base_mut()
            .with_slot(player, 1, |s, e| s.may_place(e, &stack) && !s.has_item(e));
        let slot0_free = menu
            .base_mut()
            .with_slot(player, 0, |s, e| s.may_place(e, &stack) && !s.has_item(e));
        // Java mutates the slot's own stack: a branch that bails out with `EMPTY` after a partial move still leaves the
        // moved-from slot reduced, so the remainder is written back (without `setChanged`) on that path.
        let mut write_back_only = false;
        let ok = if slot_index < player_container_start {
            menu.move_item_stack_to(player, &mut stack, player_container_start, total, true)
        } else if slot1_free {
            menu.move_item_stack_to(player, &mut stack, 1, 2, false)
        } else if slot0_free {
            menu.move_item_stack_to(player, &mut stack, 0, 1, false)
        } else if container_size == 0
            || !menu.move_item_stack_to(player, &mut stack, 2, player_container_start, false)
        {
            let player_container_end = player_container_start + 27;
            let player_hotbar_end = player_container_end + 9;
            if (player_container_end..player_hotbar_end).contains(&slot_index) {
                menu.move_item_stack_to(
                    player,
                    &mut stack,
                    player_container_start,
                    player_container_end,
                    false,
                );
            } else if (player_container_start..player_container_end).contains(&slot_index) {
                menu.move_item_stack_to(
                    player,
                    &mut stack,
                    player_container_end,
                    player_hotbar_end,
                    false,
                );
            } else {
                menu.move_item_stack_to(
                    player,
                    &mut stack,
                    player_container_end,
                    player_container_end,
                    false,
                );
            }
            write_back_only = true;
            true
        } else {
            true
        };
        if !ok {
            return ItemStack::empty();
        }
        if write_back_only {
            let rest = stack;
            menu.base_mut()
                .with_slot(player, slot_index, |s, e| s.with_item_mut(e, |t| *t = rest));
            menu.flush_container_events(player);
            return ItemStack::empty();
        }
        if stack.is_empty() {
            menu.base_mut().with_slot(player, slot_index, |s, e| {
                s.set_by_player_with(e, ItemStack::empty(), &clicked)
            });
        } else {
            let rest = stack;
            menu.base_mut().with_slot(player, slot_index, |s, e| {
                s.with_item_mut(e, |t| *t = rest);
                s.set_changed(e);
            });
        }
        menu.flush_container_events(player);
        clicked
    }
}

macro_rules! mount_menu {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        pub struct $name {
            core: MountMenuCore,
        }

        impl AbstractContainerMenu for $name {
            fn base(&self) -> &MenuBase {
                &self.core.base
            }
            fn base_mut(&mut self) -> &mut MenuBase {
                &mut self.core.base
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
            fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
                self.core.still_valid(player)
            }
            fn removed(&mut self, player: &mut dyn MenuPlayer) {
                self.base_removed(player);
                self.core.container.stop_open(player);
            }
            fn quick_move_stack(
                &mut self,
                player: &mut dyn MenuPlayer,
                slot_index: usize,
            ) -> ItemStack {
                let size = self.core.container.size();
                MountMenuCore::quick_move_stack(self, player, slot_index, size)
            }
        }
    };
}

mount_menu!(
    HorseInventoryMenu,
    "`HorseInventoryMenu(containerId, inventory, horseInventory, horse, inventoryColumns)`."
);
mount_menu!(
    NautilusInventoryMenu,
    "`NautilusInventoryMenu(containerId, inventory, nautilusInventory, nautilus, inventoryColumns)`."
);

impl HorseInventoryMenu {
    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, horse: EntityId) -> Self {
        let core = MountMenuCore::new(container_id, player, horse, |base, container, columns| {
            if columns > 0 {
                for y in 0..3 {
                    for x in 0..columns {
                        let (key, index) = container.locate((x + y * columns) as usize);
                        base.add_slot(Slot::new(key, index, 80 + x * 18, 18 + y * 18));
                    }
                }
            }
        });
        Self { core }
    }
}

impl NautilusInventoryMenu {
    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, nautilus: EntityId) -> Self {
        Self {
            core: MountMenuCore::new(container_id, player, nautilus, |_, _, _| {}),
        }
    }
}

/// `Mob.createEquipmentSlotContainer(slot)`: a one-slot container over a copy of the mob's equipment stack that is
/// written back (through [`MountInventory::set_equipment_slot_item`]) only when it was touched.
pub(super) struct EquipmentSlotContainer {
    pub stack: ItemStack,
    pub dirty: bool,
}

impl crate::world::container::Container for EquipmentSlotContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        1
    }
    fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        if slot == 0 {
            &self.stack
        } else {
            crate::world::container::empty_stack()
        }
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        if slot == 0 {
            self.dirty = true;
            Some(&mut self.stack)
        } else {
            None
        }
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        if slot != 0 {
            return ItemStack::empty();
        }
        self.dirty = true;
        self.stack.split(count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        let max = self.get_max_stack_size();
        self.remove_item(slot, max)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if slot == 0 {
            self.stack = stack;
            self.dirty = true;
        }
    }
    fn clear_content(&mut self) {
        let max = self.get_max_stack_size();
        self.remove_item(0, max);
    }
    fn set_changed(&mut self) {}
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
    fn get_max_stack_size(&self) -> i32 {
        99
    }
}
