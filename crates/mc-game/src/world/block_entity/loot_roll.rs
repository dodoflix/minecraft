//! Loot rolls of the spawner family that draw from the level's own random like vanilla (`Level.random`).
//!
//! `LootTable.getRandomItems(params)` draws from the table's `random_sequence`, or from `level.getRandom()` when it has
//! none; `getRandomItems(params, random)` draws from the given random. The loot module only has `&ServerLevel`, so it
//! rolls on a copy of the level random; these helpers roll on a shared handle instead and return the advanced state for
//! the caller to store back once its `LootParams` (which borrow the level) are dropped.

use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;
use crate::world::level::storage::loot::{LootParams, LootTable};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, PositionalRandomFactory, RandomSource};
use parking_lot::Mutex;
use std::sync::Arc;

/// A handle on the level's random that several owners can draw from.
struct SharedRandom(Arc<Mutex<LegacyRandomSource>>);

macro_rules! draw {
    ($self:ident . $m:ident ( $($a:expr),* )) => { $self.0.lock().$m($($a),*) };
}

impl RandomSource for SharedRandom {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        draw!(self.fork())
    }
    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        draw!(self.fork_positional())
    }
    fn set_seed(&mut self, seed: i64) {
        draw!(self.set_seed(seed))
    }
    fn next_int(&mut self) -> i32 {
        draw!(self.next_int())
    }
    fn next_int_bounded(&mut self, bound: i32) -> i32 {
        draw!(self.next_int_bounded(bound))
    }
    fn next_long(&mut self) -> i64 {
        draw!(self.next_long())
    }
    fn next_boolean(&mut self) -> bool {
        draw!(self.next_boolean())
    }
    fn next_float(&mut self) -> f32 {
        draw!(self.next_float())
    }
    fn next_double(&mut self) -> f64 {
        draw!(self.next_double())
    }
    fn next_gaussian(&mut self) -> f64 {
        draw!(self.next_gaussian())
    }
}

/// Rolls `table` with `params`. With `explicit_level_random` (`getRandomItems(params, level.getRandom())`) or when the
/// table has no random sequence, the roll consumes the level random: the advanced state is returned and must be stored
/// into `level.tick_state.random` by the caller.
pub fn roll(params: &LootParams, table: &LootTable, explicit_level_random: bool) -> (Vec<ItemStack>, Option<LegacyRandomSource>) {
    if !explicit_level_random && table.random_sequence().is_some() {
        return (table.get_random_items(params), None);
    }
    let shared = Arc::new(Mutex::new(params.level().tick_state.random.clone()));
    let items = table.get_random_items_with_random(params, Box::new(SharedRandom(shared.clone())));
    let advanced = shared.lock().clone();
    (items, Some(advanced))
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
