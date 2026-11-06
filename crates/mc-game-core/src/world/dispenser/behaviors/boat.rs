//! `BoatDispenseItemBehavior`: boats, chest boats and rafts (one instance per entity type).

use crate::world::dispenser::{BlockSource, DispenseTemplate, Templated, default_behavior};
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::entity::spawn_config::apply_default_stack_config;
use crate::world::item::ItemStack;
use crate::world::level::BlockGetter;
use crate::world::level::material::FluidStateExt;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

struct BoatDispense {
    ty: EntityType,
}

impl DispenseTemplate for BoatDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let direction = source.facing();
        let center = source.center();
        let just_outside_dispenser = 0.5625 + f64::from(self.ty.data().width) / 2.0;
        let spawn_x = center.x + f64::from(direction.get_step_x()) * just_outside_dispenser;
        let spawn_y = center.y + f64::from(direction.get_step_y()) * f64::from(1.125f32);
        let spawn_z = center.z + f64::from(direction.get_step_z()) * just_outside_dispenser;
        let front_pos = source.pos.relative(direction);
        let y_offset = if source.level.get_fluid_state(front_pos).is_tag("water") {
            1.0
        } else {
            if !source.level.get_block_state(front_pos).is_air()
                || !source
                    .level
                    .get_fluid_state(front_pos.below())
                    .is_tag("water")
            {
                return default_behavior().dispense(source, dispensed);
            }
            0.0
        };
        if let Some(mut boat) = factory::create(
            self.ty,
            source.level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Dispenser,
                ignore_checks: false,
            },
        ) {
            // `AbstractBoat.setInitialPos(x, y, z)`
            boat.set_pos(spawn_x, spawn_y + y_offset, spawn_z);
            let at = boat.position();
            let base = boat.base_mut();
            base.xo = at.x;
            base.yo = at.y;
            base.zo = at.z;
            apply_default_stack_config(source.level, boat.as_mut(), &dispensed, None);
            boat.set_y_rot(direction.to_y_rot());
            source.level.add_fresh_entity(boat);
            dispensed.shrink(1);
        }
        dispensed
    }
}

crate::register_dispense_behavior_per_item!(
    &[
        "minecraft:oak_boat",
        "minecraft:spruce_boat",
        "minecraft:birch_boat",
        "minecraft:jungle_boat",
        "minecraft:dark_oak_boat",
        "minecraft:acacia_boat",
        "minecraft:cherry_boat",
        "minecraft:mangrove_boat",
        "minecraft:poplar_boat",
        "minecraft:pale_oak_boat",
        "minecraft:bamboo_raft",
        "minecraft:oak_chest_boat",
        "minecraft:spruce_chest_boat",
        "minecraft:birch_chest_boat",
        "minecraft:jungle_chest_boat",
        "minecraft:dark_oak_chest_boat",
        "minecraft:acacia_chest_boat",
        "minecraft:cherry_chest_boat",
        "minecraft:mangrove_chest_boat",
        "minecraft:poplar_chest_boat",
        "minecraft:pale_oak_chest_boat",
        "minecraft:bamboo_chest_raft",
    ],
    |name| Box::new(Templated(BoatDispense {
        ty: EntityType::by_name(name).expect("boat entity type named like its item")
    }))
);
