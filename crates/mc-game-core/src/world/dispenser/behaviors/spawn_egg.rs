//! `SpawnEggItemBehavior`: the default for a spawn egg with an `entity_data` component.

use crate::server::level::ServerLevel;
use crate::world::dispenser::{BlockSource, DispenseTemplate, Templated};
use crate::world::entity::Entity;
use crate::world::entity::spawn_config::{apply_default_stack_config, spawn_at};
use crate::world::item::ItemStack;
use crate::world::item::spawn_egg_item::get_type;
use crate::world::level::gameevent::GameEventContext;
use mc_core::direction::Direction;
use mc_data::GameEvent;
use mc_data::enums::EntitySpawnReason;

pub struct SpawnEggDispense;

impl DispenseTemplate for SpawnEggDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let direction = source.facing();
        let Some(ty) = get_type(&dispensed) else {
            return dispensed;
        };
        let stack = dispensed.clone();
        let spawned = spawn_at(
            source.level,
            ty,
            Some(&mut |level: &mut ServerLevel, entity: &mut dyn Entity| {
                apply_default_stack_config(level, entity, &stack, None);
            }),
            source.pos.relative(direction),
            EntitySpawnReason::Dispenser,
            direction != Direction::Up,
            false,
        );
        if spawned.is_none() {
            return dispensed;
        }
        dispensed.shrink(1);
        let pos = source.pos;
        source.level.game_event_at_block(
            GameEvent::ENTITY_PLACE,
            pos,
            &GameEventContext::of_entity(None),
        );
        dispensed
    }
}

pub static SPAWN_EGG: Templated<SpawnEggDispense> = Templated(SpawnEggDispense);
