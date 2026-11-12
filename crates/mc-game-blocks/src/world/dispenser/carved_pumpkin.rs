//! `Blocks.CARVED_PUMPKIN`: places the pumpkin in front when it completes a golem pattern, else equips it.

use crate::world::level::block::impls::carved_pumpkin_block::can_spawn_golem;
use mc_data::GameEvent;
use mc_data::block::Block;
use mc_game_core::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, dispense_equipment,
    play_optional_sound,
};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::{LevelAccessor, LevelReader};

struct CarvedPumpkinDispense(OptionalState);

impl DispenseTemplate for CarvedPumpkinDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let target = source.pos.relative(source.facing());
        if source.level.is_empty_block(target) && can_spawn_golem(&*source.level, target) {
            source
                .level
                .set_block_and_update(target, Block::CARVED_PUMPKIN.default_state());
            source.level.game_event_at_block(
                GameEvent::BLOCK_PLACE,
                target,
                &GameEventContext::of_entity(None),
            );
            dispensed.shrink(1);
            self.0.set_success(true);
        } else {
            self.0
                .set_success(dispense_equipment(source, &mut dispensed));
        }
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

mc_game_core::register_dispense_behavior!(&["minecraft:carved_pumpkin"], |_| Box::new(Templated(
    CarvedPumpkinDispense(OptionalState::new())
)));
