//! `DefaultDispenseItemBehavior`.

use super::{BlockSource, DispenseTemplate, get_dispense_position};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::item::ItemEntity;
use crate::world::item::ItemStack;
use crate::world::level::LevelAccessor;
use mc_core::direction::{Axis, Direction};
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;

const DEFAULT_ACCURACY: i32 = 6;

/// The plain `DefaultDispenseItemBehavior` (drops the item).
pub struct DefaultDispense;

impl DispenseTemplate for DefaultDispense {}

/// `DefaultDispenseItemBehavior.execute` (what `super.execute(..)` calls): spawn one item in front.
pub fn default_execute(source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
    let direction = source.facing();
    let position = get_dispense_position(source);
    let stack = dispensed.split(1);
    spawn_item(source.level, stack, DEFAULT_ACCURACY, direction, position);
    dispensed
}

/// `DefaultDispenseItemBehavior.spawnItem(level, itemStack, accuracy, direction, position)`.
pub fn spawn_item(
    level: &mut ServerLevel,
    stack: ItemStack,
    accuracy: i32,
    direction: Direction,
    position: Vec3,
) {
    let spawn_x = position.x;
    let mut spawn_y = position.y;
    let spawn_z = position.z;
    if direction.get_axis() == Axis::Y {
        spawn_y -= 0.125;
    } else {
        spawn_y -= 0.15625;
    }
    let mut item_entity = ItemEntity::with_stack(level, spawn_x, spawn_y, spawn_z, stack);
    let random = level.random();
    let pow = random.next_double() * 0.1 + 0.2;
    let spread = 0.0172275 * f64::from(accuracy);
    let dx = random.triangle(f64::from(direction.get_step_x()) * pow, spread);
    let dy = random.triangle(0.2, spread);
    let dz = random.triangle(f64::from(direction.get_step_z()) * pow, spread);
    item_entity.base_mut().set_delta_movement_xyz(dx, dy, dz);
    level.add_fresh_entity(Box::new(item_entity));
}

/// `playDefaultSound(source)`: level event 1000 (dispenser click).
pub fn play_default_sound(source: &mut BlockSource<'_>) {
    let pos = source.pos;
    source.level.level_event(None, 1000, pos, 0);
}

/// `playDefaultAnimation(source, direction)`: level event 2000 (smoke particles).
pub fn play_default_animation(source: &mut BlockSource<'_>, direction: Direction) {
    let pos = source.pos;
    source
        .level
        .level_event(None, 2000, pos, direction.get_3d_data_value() as i32);
}

/// `consumeWithRemainder(source, dispensed, remainder)`: the remainder replaces a used-up stack, else goes into the
/// dispenser or is dropped.
pub fn consume_with_remainder(
    source: &mut BlockSource<'_>,
    mut dispensed: ItemStack,
    remainder: ItemStack,
) -> ItemStack {
    dispensed.shrink(1);
    if dispensed.is_empty() {
        return remainder;
    }
    add_to_inventory_or_dispense(source, remainder);
    dispensed
}

/// `addToInventoryOrDispense(source, itemStack)`.
fn add_to_inventory_or_dispense(source: &mut BlockSource<'_>, stack: ItemStack) {
    let remainder = source.block_entity.insert_item(stack);
    if !remainder.is_empty() {
        let direction = source.facing();
        let position = get_dispense_position(source);
        spawn_item(
            source.level,
            remainder,
            DEFAULT_ACCURACY,
            direction,
            position,
        );
        play_default_sound(source);
        play_default_animation(source, direction);
    }
}

/// `OptionalDispenseItemBehavior.success`: shared by every dispense through the same behavior instance and never reset
/// by vanilla (a behavior that fails once keeps reporting failure until it sets `true` again).
pub struct OptionalState(std::sync::atomic::AtomicBool);

impl OptionalState {
    pub const fn new() -> Self {
        Self(std::sync::atomic::AtomicBool::new(true))
    }
    pub fn is_success(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn set_success(&self, success: bool) {
        self.0.store(success, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for OptionalState {
    fn default() -> Self {
        Self::new()
    }
}

/// `OptionalDispenseItemBehavior.playSound`: click (1000) on success, failure click (1001) otherwise.
pub fn play_optional_sound(source: &mut BlockSource<'_>, success: bool) {
    let pos = source.pos;
    source
        .level
        .level_event(None, if success { 1000 } else { 1001 }, pos, 0);
}
