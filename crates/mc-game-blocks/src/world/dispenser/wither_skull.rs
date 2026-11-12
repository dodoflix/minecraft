//! `Items.WITHER_SKELETON_SKULL`: places the skull in front (checking the wither pattern), else equips it.

use crate::world::level::block::impls::wither_skull_block::{can_spawn_mob, check_spawn};
use mc_data::GameEvent;
use mc_data::block::Block;
use mc_data::props;
use mc_game_core::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, dispense_equipment,
    play_optional_sound,
};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::rotation_segment::convert_direction_to_segment;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::{LevelAccessor, LevelReader};

struct WitherSkullDispense(OptionalState);

impl DispenseTemplate for WitherSkullDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        let direction = source.facing();
        let target = source.pos.relative(direction);
        if source.level.is_empty_block(target) && can_spawn_mob(source.level, target, &dispensed) {
            source.level.set_block_and_update(
                target,
                Block::WITHER_SKELETON_SKULL
                    .default_state()
                    .set(props::ROTATION_16, convert_direction_to_segment(direction)),
            );
            source.level.game_event_at_block(
                GameEvent::BLOCK_PLACE,
                target,
                &GameEventContext::of_entity(None),
            );
            check_spawn(source.level, target);
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

mc_game_core::register_dispense_behavior!(&["minecraft:wither_skeleton_skull"], |_| Box::new(
    Templated(WitherSkullDispense(OptionalState::new()))
));
