//! Loot rolls of the spawner family that draw from the level's own random like vanilla (`Level.random`).
//!
//! `LootTable.getRandomItems(params)` draws from the table's `random_sequence`, or from `level.getRandom()` when it has
//! none; `getRandomItems(params, random)` draws from the given random.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::item::ItemStack;
use crate::world::level::LevelAccessor;
use crate::world::level::storage::loot::{LootParams, LootTable};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

/// Rolls `table` with `params`: `explicit_level_random` is `getRandomItems(params, level.getRandom())`, otherwise the
/// table's own random sequence / the level random decide (`LevelRandom` is a shared handle, so the level's stream advances).
pub fn roll(params: &LootParams, table: &LootTable, explicit_level_random: bool) -> Vec<ItemStack> {
    if explicit_level_random {
        table.get_random_items_with_random(params, Box::new(params.level().tick_state.random.clone()))
    } else {
        table.get_random_items(params)
    }
}

/// `DefaultDispenseItemBehavior.spawnItem(level, stack, accuracy, direction, position)`.
pub fn spawn_item(level: &mut ServerLevel, stack: ItemStack, accuracy: i32, direction: Direction, position: Vec3) {
    use crate::world::entity::item::ItemEntity;
    let spawn_y = position.y - if direction.get_axis() == mc_core::direction::Axis::Y { 0.125 } else { 0.15625 };
    let mut item = ItemEntity::with_stack(level, position.x, spawn_y, position.z, stack);
    let random = level.random();
    let pow = random.next_double() * 0.1 + 0.2;
    let spread = 0.0172275 * f64::from(accuracy);
    let dx = random.triangle(f64::from(direction.get_step_x()) * pow, spread);
    let dy = random.triangle(0.2, spread);
    let dz = random.triangle(f64::from(direction.get_step_z()) * pow, spread);
    item.base_mut().set_delta_movement(Vec3::new(dx, dy, dz));
    level.add_fresh_entity(Box::new(item));
}

/// `Vec3.atBottomCenterOf(pos).relative(UP, 1.2)`: where rewards pop out.
pub fn reward_eject_position(pos: BlockPos) -> Vec3 {
    Vec3::at_bottom_center_of(&pos).relative(Direction::Up, 1.2)
}
