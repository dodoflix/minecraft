//! `net.minecraft.core.dispenser.*` + `DispenserBlock`'s behavior registry.
//!
//! Vanilla behaviors are singletons with a mutable `success` field (`OptionalDispenseItemBehavior`) whose value leaks
//! from one dispense to the next; the Rust behaviors keep that state in an atomic on the shared instance too.
//! A behavior implements [`DispenseTemplate`] (`execute`/`playSound`/`playAnimation` overrides of
//! `DefaultDispenseItemBehavior`, defaults = the base class) and is wrapped in [`Templated`], which provides the final
//! `dispense` = execute, sound, animation.
//!
//! The dispenser holds its block entity (taken out of the chunk while it dispenses) and the stack in the chosen slot
//! is handed to the behavior by value; the behavior returns the stack to store back (vanilla mutates the stack object
//! stored in the slot and returns it or a replacement). During a dispense the dispenser block entity is therefore not
//! visible through the level (comparators reading it mid-dispense see no container).

use crate::server::level::ServerLevel;
use crate::world::block_entity::DispenserBlockEntity;
use crate::world::item::ItemStack;
use linkme::distributed_slice;
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::Item;
use mc_data::block::BlockState;
use mc_data::props;
use std::collections::HashMap;
use std::sync::OnceLock;

mod behaviors;
mod default_behavior;
pub use default_behavior::{
    DefaultDispense, OptionalState, consume_with_remainder, default_execute,
    play_default_animation, play_default_sound, play_optional_sound, spawn_item,
};

/// `BlockSource(level, pos, state, blockEntity)`.
pub struct BlockSource<'a> {
    pub level: &'a mut ServerLevel,
    pub pos: BlockPos,
    pub state: BlockState,
    pub block_entity: &'a mut DispenserBlockEntity,
}

impl BlockSource<'_> {
    /// `state().getValue(DispenserBlock.FACING)`.
    pub fn facing(&self) -> Direction {
        self.state.get(props::FACING)
    }

    /// `center()` = `Vec3.atCenterOf(pos)`.
    pub fn center(&self) -> Vec3 {
        Vec3::at_center_of(&self.pos)
    }
}

/// `DispenseItemBehavior`.
pub trait DispenseItemBehavior: Send + Sync {
    fn dispense(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack;
}

/// The overridable parts of `DefaultDispenseItemBehavior`.
pub trait DispenseTemplate: Send + Sync {
    fn execute(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack {
        default_execute(source, dispensed)
    }
    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_default_sound(source);
    }
    fn play_animation(&self, source: &mut BlockSource<'_>, direction: Direction) {
        play_default_animation(source, direction);
    }
}

/// `DefaultDispenseItemBehavior.dispense` (final): `execute`, then sound, then animation.
pub struct Templated<T>(pub T);

impl<T: DispenseTemplate> DispenseItemBehavior for Templated<T> {
    fn dispense(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack {
        let result = self.0.execute(source, dispensed);
        self.0.play_sound(source);
        let facing = source.facing();
        self.0.play_animation(source, facing);
        result
    }
}

/// `DispenserBlock.getDispensePosition(source)` (scale 0.7, no offset).
pub fn get_dispense_position(source: &BlockSource<'_>) -> Vec3 {
    get_dispense_position_with(source, 0.7, Vec3::ZERO)
}

/// `DispenserBlock.getDispensePosition(source, scale, offset)`.
pub fn get_dispense_position_with(source: &BlockSource<'_>, scale: f64, offset: Vec3) -> Vec3 {
    let direction = source.facing();
    let center = source.center();
    Vec3::new(
        center.x + (scale * f64::from(direction.get_step_x()) + offset.x),
        center.y + (scale * f64::from(direction.get_step_y()) + offset.y),
        center.z + (scale * f64::from(direction.get_step_z()) + offset.z),
    )
}

// ---------------------------------------------------------------------------------------------------- registry

/// Registers one behavior for item names: `items` all share the instance made by `create` (called once with the first
/// name), or each get their own (`per_item`, e.g. boats/minecarts which differ by entity type).
pub struct DispenseRegistration {
    pub items: &'static [&'static str],
    pub per_item: bool,
    pub create: fn(&'static str) -> Box<dyn DispenseItemBehavior>,
}

#[distributed_slice]
pub static DISPENSE_BEHAVIORS: [DispenseRegistration];

/// `DispenserBlock.registerBehavior(item, behavior)` for a shared behavior instance.
#[macro_export]
macro_rules! register_dispense_behavior {
    ($items:expr, $create:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::world::dispenser::DISPENSE_BEHAVIORS)]
            static REG: $crate::world::dispenser::DispenseRegistration =
                $crate::world::dispenser::DispenseRegistration {
                    items: $items,
                    per_item: false,
                    create: $create,
                };
        };
    };
}
/// `DispenserBlock.registerBehavior(item, behavior)` with one instance per item (the creator gets the item name).
#[macro_export]
macro_rules! register_dispense_behavior_per_item {
    ($items:expr, $create:expr) => {
        const _: () = {
            #[::linkme::distributed_slice($crate::world::dispenser::DISPENSE_BEHAVIORS)]
            static REG: $crate::world::dispenser::DispenseRegistration =
                $crate::world::dispenser::DispenseRegistration {
                    items: $items,
                    per_item: true,
                    create: $create,
                };
        };
    };
}
pub use crate::{register_dispense_behavior, register_dispense_behavior_per_item};

type Registry = HashMap<Item, &'static dyn DispenseItemBehavior>;

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// `DispenserBlock.DISPENSER_REGISTRY` (later registrations win, as `Map.put`; registration order across files is link
/// order, files never register the same item twice).
fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let mut map = Registry::new();
        for reg in DISPENSE_BEHAVIORS.iter() {
            let mut shared: Option<&'static dyn DispenseItemBehavior> = None;
            for name in reg.items {
                let Some(item) = Item::by_name(name) else {
                    log::warn!("dispenser behavior registered for unknown item {name}");
                    continue;
                };
                let behavior: &'static dyn DispenseItemBehavior = if reg.per_item {
                    Box::leak((reg.create)(name))
                } else {
                    *shared.get_or_insert_with(|| Box::leak((reg.create)(name)))
                };
                map.insert(item, behavior);
            }
        }
        map
    })
}

static DEFAULT_BEHAVIOR: Templated<DefaultDispense> = Templated(DefaultDispense);

/// The shared `DispenserBlock.DEFAULT_BEHAVIOR`.
pub fn default_behavior() -> &'static dyn DispenseItemBehavior {
    &DEFAULT_BEHAVIOR
}

/// `DispenserBlock.getDispenseMethod(level, itemStack)`.
pub fn get_dispense_method(
    level: &ServerLevel,
    stack: &ItemStack,
) -> &'static dyn DispenseItemBehavior {
    use crate::world::level::LevelReader;
    if !crate::world::item::is_item_enabled(stack, level.enabled_features()) {
        return default_behavior();
    }
    if let Some(behavior) = registry().get(&stack.item()) {
        return *behavior;
    }
    get_default_dispense_method(stack)
}

/// `DispenserBlock.getDefaultDispenseMethod(itemStack)`.
fn get_default_dispense_method(_stack: &ItemStack) -> &'static dyn DispenseItemBehavior {
    default_behavior()
}
