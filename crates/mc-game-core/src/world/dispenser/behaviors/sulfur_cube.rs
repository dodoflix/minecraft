//! `SulfurCubeBlockDispenseItemBehavior`: the default for `#sulfur_cube_swallowable` items.

use crate::server::level::ServerLevel;
use crate::world::dispenser::{BlockSource, DispenseTemplate, Templated, default_execute};
use crate::world::item::ItemStack;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::EntityType;

/// `SulfurCubeBlockDispenseItemBehavior.dispenseBlock(level, pos, dispensed)`: the first sulfur cube at `pos` that
/// swallows the item takes one of the stack.
pub fn dispense_block(level: &mut ServerLevel, pos: BlockPos, dispensed: &mut ItemStack) -> bool {
    let cubes = level.get_entities_of_class(
        &AABB::from_block_pos(&pos),
        |e| e.get_type() == EntityType::SULFUR_CUBE,
        |_| true,
    );
    for id in cubes {
        let swallowed = level.with_entity(id, |level, cube| {
            cube.dispenser_swallow_item(level, dispensed)
        });
        if swallowed == Some(true) {
            dispensed.shrink(1);
            return true;
        }
    }
    false
}

pub struct SulfurCubeDispense;

impl DispenseTemplate for SulfurCubeDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let pos = source.pos.relative(source.facing());
        if dispense_block(source.level, pos, &mut dispensed) {
            dispensed
        } else {
            default_execute(source, dispensed)
        }
    }
}

pub static SULFUR_CUBE: Templated<SulfurCubeDispense> = Templated(SulfurCubeDispense);
