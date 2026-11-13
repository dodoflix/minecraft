//! `MinecartDispenseItemBehavior`: the six minecart items (one instance per entity type); its sound (level event 1000) is the default one.

use crate::world::dispenser::{BlockSource, DispenseTemplate, Templated, default_behavior};
use crate::world::entity::vehicle::minecart::abstract_minecart::{create_minecart, rail_shape};
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use crate::world::level::block::impls::base_rail_block::is_slope;
use crate::world::level::block::state::BlockStateExt;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

struct MinecartDispense {
    ty: EntityType,
}

impl DispenseTemplate for MinecartDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let direction = source.facing();
        let center = source.center();
        let spawn_x = center.x + f64::from(direction.get_step_x()) * 1.125;
        let spawn_y = mth::floor_f64(center.y) as f64 + f64::from(direction.get_step_y());
        let spawn_z = center.z + f64::from(direction.get_step_z()) * 1.125;
        let front = source.pos.relative(direction);
        let block_front = source.level.get_block_state(front);
        let y_offset = if block_front.is_tag("rails") {
            if is_slope(rail_shape(block_front)) {
                0.6
            } else {
                0.1
            }
        } else {
            if !block_front.is_air() {
                return default_behavior().dispense(source, dispensed);
            }
            let block_below = source.level.get_block_state(front.below());
            if !block_below.is_tag("rails") {
                return default_behavior().dispense(source, dispensed);
            }
            if direction != Direction::Down && is_slope(rail_shape(block_below)) {
                -0.4
            } else {
                -0.9
            }
        };
        if let Some(minecart) = create_minecart(
            source.level,
            spawn_x,
            spawn_y + y_offset,
            spawn_z,
            self.ty,
            EntitySpawnReason::Dispenser,
            &dispensed,
            None,
        ) {
            source.level.add_fresh_entity(minecart);
            dispensed.shrink(1);
        }
        dispensed
    }
}

crate::register_dispense_behavior_per_item!(
    &[
        "minecraft:minecart",
        "minecraft:chest_minecart",
        "minecraft:furnace_minecart",
        "minecraft:tnt_minecart",
        "minecraft:hopper_minecart",
        "minecraft:command_block_minecart",
    ],
    |name| Box::new(Templated(MinecartDispense {
        ty: EntityType::by_name(name).expect("minecart entity type named like its item")
    }))
);
