//! `Items.ARMOR_STAND` (anonymous `DefaultDispenseItemBehavior` of `DispenseItemBehavior.bootStrap`).

use crate::world::dispenser::{BlockSource, DispenseTemplate, Templated};
use crate::world::entity::Entity;
use crate::world::entity::spawn_config::{apply_default_stack_config, create_at};
use crate::world::item::ItemStack;
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

struct ArmorStandDispense;

impl DispenseTemplate for ArmorStandDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let direction = source.facing();
        let pos = source.pos.relative(direction);
        let y_rot = direction.to_y_rot();
        let stack = dispensed.clone();
        let mut config = |level: &mut crate::server::level::ServerLevel,
                          entity: &mut dyn Entity| {
            entity.set_y_rot(y_rot);
            apply_default_stack_config(level, entity, &stack, None);
        };
        let spawned = create_at(
            source.level,
            EntityType::ARMOR_STAND,
            Some(&mut config),
            pos,
            EntitySpawnReason::Dispenser,
            false,
            false,
        );
        if let Some(entity) = spawned {
            source.level.add_fresh_entity_with_passengers(vec![entity]);
            dispensed.shrink(1);
        }
        dispensed
    }
}

crate::register_dispense_behavior!(&["minecraft:armor_stand"], |_| Box::new(Templated(
    ArmorStandDispense
)));
