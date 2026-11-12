//! `Items.HONEYCOMB`: waxes the block in front, else drops the honeycomb.

use crate::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, default_execute, play_optional_sound,
};
use crate::world::item::ItemStack;
use crate::world::item::honeycomb_item::get_waxed;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_data::SoundEvent;
use mc_protocol::packets::play::SoundSource;

struct HoneycombDispense(OptionalState);

impl DispenseTemplate for HoneycombDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let pos = source.pos.relative(source.facing());
        let level = &mut *source.level;
        let Some(waxed) = get_waxed(level.get_block_state(pos)) else {
            return default_execute(source, dispensed);
        };
        level.set_block_and_update(pos, waxed);
        level.level_event(None, 3003, pos, 0);
        level.play_sound(
            None,
            pos,
            SoundEvent::ITEM_HONEYCOMB_WAX_ON,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        dispensed.shrink(1);
        self.0.set_success(true);
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

crate::register_dispense_behavior!(&["minecraft:honeycomb"], |_| Box::new(Templated(
    HoneycombDispense(OptionalState::new())
)));
