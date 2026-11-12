//! `EquipmentDispenseItemBehavior`: the default for items with an `equippable` component.

use crate::world::dispenser::{BlockSource, DispenseTemplate, Templated, default_execute};
use crate::world::entity::mob::equipment::{can_equip_with_dispenser, set_guaranteed_drop};
use crate::world::item::ItemStack;
use mc_core::phys::AABB;

/// `EquipmentDispenseItemBehavior.dispenseEquipment(source, dispensed)`: equips one item on the first living entity in
/// front that accepts it.
pub fn dispense_equipment(source: &mut BlockSource<'_>, dispensed: &mut ItemStack) -> bool {
    let pos = source.pos.relative(source.facing());
    let level = &mut *source.level;
    let targets = level.get_entities_of_class(
        &AABB::from_block_pos(&pos),
        |e| e.living().is_some(),
        |e| can_equip_with_dispenser(e, dispensed),
    );
    let Some(&target) = targets.first() else {
        return false;
    };
    level.with_entity(target, |level, target| {
        let slot = target.get_equipment_slot_for_item(dispensed);
        let equip = dispensed.split(1);
        target.set_item_slot(level, slot, equip);
        if target.is_mob() {
            set_guaranteed_drop(target, slot);
            target.set_persistence_required();
        }
    });
    true
}

pub struct EquipmentDispense;

impl DispenseTemplate for EquipmentDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        if dispense_equipment(source, &mut dispensed) {
            dispensed
        } else {
            default_execute(source, dispensed)
        }
    }
}

pub static EQUIPMENT: Templated<EquipmentDispense> = Templated(EquipmentDispense);
