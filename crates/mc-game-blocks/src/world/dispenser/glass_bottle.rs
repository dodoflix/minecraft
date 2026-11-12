//! `Items.GLASS_BOTTLE`: fills from a honey-full beehive or from water in front, else drops the bottle.

use crate::world::level::block::impls::beehive_block::{
    MAX_HONEY_LEVELS, release_bees_and_reset_honey_level,
};
use mc_core::pos::BlockPos;
use mc_data::{GameEvent, Item, Potion, props};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::BeeReleaseStatus;
use mc_game_core::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, consume_with_remainder,
    default_execute, play_optional_sound,
};
use mc_game_core::world::item::{ItemStack, create_potion_item_stack};
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::material::FluidStateExt;

struct GlassBottleDispense(OptionalState);

/// `takeLiquid(source, dispensed, filled)`.
fn take_liquid(source: &mut BlockSource<'_>, dispensed: ItemStack, filled: ItemStack) -> ItemStack {
    let pos = source.pos;
    source.level.game_event_at_block(
        GameEvent::FLUID_PICKUP,
        pos,
        &GameEventContext::of_entity(None),
    );
    consume_with_remainder(source, dispensed, filled)
}

impl DispenseTemplate for GlassBottleDispense {
    fn execute(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack {
        self.0.set_success(false);
        let target: BlockPos = source.pos.relative(source.facing());
        let state = source.level.get_block_state(target);
        if state.is_tag("beehives")
            && state.has_property(props::LEVEL_HONEY)
            && state.get(props::LEVEL_HONEY) >= MAX_HONEY_LEVELS
        {
            release_bees_and_reset_honey_level(
                source.level,
                state,
                target,
                BeeReleaseStatus::BeeReleased,
            );
            self.0.set_success(true);
            take_liquid(source, dispensed, ItemStack::new(Item::HONEY_BOTTLE))
        } else if is_water(source.level, target) {
            self.0.set_success(true);
            take_liquid(
                source,
                dispensed,
                create_potion_item_stack(Item::POTION, Potion::WATER),
            )
        } else {
            default_execute(source, dispensed)
        }
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

fn is_water(level: &ServerLevel, pos: BlockPos) -> bool {
    level.get_fluid_state(pos).is_tag("water")
}

mc_game_core::register_dispense_behavior!(&["minecraft:glass_bottle"], |_| Box::new(Templated(
    GlassBottleDispense(OptionalState::new())
)));
