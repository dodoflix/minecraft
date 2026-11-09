//! `ShulkerBoxDispenseBehavior`: places the block in front (`BlockItem.place` with a `DirectionalPlaceContext`).

use crate::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, play_optional_sound,
};
use crate::world::item::{BlockItem, BlockPlaceContext, ItemStack};
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::direction::Direction;

struct ShulkerBoxDispense(OptionalState);

impl DispenseTemplate for ShulkerBoxDispense {
    fn execute(&self, source: &mut BlockSource<'_>, dispensed: ItemStack) -> ItemStack {
        self.0.set_success(false);
        let item = dispensed.item();
        if item.block().is_none() {
            return dispensed;
        }
        let facing = source.facing();
        let relative_pos = source.pos.relative(facing);
        let clicked_face = if source.level.is_empty_block(relative_pos.below()) {
            facing
        } else {
            Direction::Up
        };
        let state_at_pos = BlockGetter::get_block_state(&*source.level, relative_pos);
        let mut context = BlockPlaceContext::directional(
            relative_pos,
            facing,
            dispensed,
            clicked_face,
            state_at_pos,
        );
        let result = BlockItem::for_item(item).place(&mut context, source.level, None);
        self.0.set_success(result.consumes_action());
        context.use_on.item_stack
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

crate::register_dispense_behavior!(
    &[
        "minecraft:shulker_box",
        "minecraft:white_shulker_box",
        "minecraft:orange_shulker_box",
        "minecraft:magenta_shulker_box",
        "minecraft:light_blue_shulker_box",
        "minecraft:yellow_shulker_box",
        "minecraft:lime_shulker_box",
        "minecraft:pink_shulker_box",
        "minecraft:gray_shulker_box",
        "minecraft:light_gray_shulker_box",
        "minecraft:cyan_shulker_box",
        "minecraft:purple_shulker_box",
        "minecraft:blue_shulker_box",
        "minecraft:brown_shulker_box",
        "minecraft:green_shulker_box",
        "minecraft:red_shulker_box",
        "minecraft:black_shulker_box",
    ],
    |_| Box::new(Templated(ShulkerBoxDispense(OptionalState::new())))
);
