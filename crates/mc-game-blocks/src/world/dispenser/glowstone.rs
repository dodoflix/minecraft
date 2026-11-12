//! `Items.GLOWSTONE`: charges the respawn anchor in front, else drops the item.

use crate::world::level::block::impls::respawn_anchor_block::charge;
use mc_data::block::Block;
use mc_data::props;
use mc_game_core::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, default_execute, play_optional_sound,
};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::BlockStateExt;

struct GlowstoneDispense(OptionalState);

impl DispenseTemplate for GlowstoneDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let pos = source.pos.relative(source.facing());
        let state = source.level.get_block_state(pos);
        self.0.set_success(true);
        if state.is_block(Block::RESPAWN_ANCHOR) {
            if state.get(props::RESPAWN_ANCHOR_CHARGES) != 4 {
                charge(None, source.level, pos, state);
                dispensed.shrink(1);
            } else {
                self.0.set_success(false);
            }
            dispensed
        } else {
            default_execute(source, dispensed)
        }
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

mc_game_core::register_dispense_behavior!(&["minecraft:glowstone"], |_| Box::new(Templated(
    GlowstoneDispense(OptionalState::new())
)));
