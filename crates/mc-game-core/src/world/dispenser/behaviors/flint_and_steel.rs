//! `FlintAndSteelDispenseItemBehavior`: sets fire, lights a campfire, candle or candle cake, primes TNT or an exploding
//! sulfur cube in front, and damages the tool.

use crate::server::level::ServerLevel;
use crate::world::dispenser::{
    BlockSource, DispenseTemplate, OptionalState, Templated, play_optional_sound,
};
use crate::world::item::{ItemStack, hurt_and_break_stack};
use crate::world::level::block::impls::{
    base_fire_block, campfire_block, candle_block, candle_cake_block, tnt_block,
};
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::{EntityType, GameEvent, props};

struct FlintAndSteelDispense(OptionalState);

/// `tryIgniteExplosiveEntities(level, pos)`: primes the first sulfur cube at `pos` that can explode.
fn try_ignite_explosive_entities(level: &mut ServerLevel, pos: BlockPos) -> bool {
    let cubes = level.get_entities_of_class(
        &AABB::from_block_pos(&pos),
        |e| e.get_type() == EntityType::SULFUR_CUBE && e.sulfur_cube_can_explode(),
        |_| true,
    );
    let Some(&first) = cubes.first() else {
        return false;
    };
    level.with_entity(first, |level, cube| {
        cube.sulfur_cube_prime_time(level, false)
    });
    true
}

impl DispenseTemplate for FlintAndSteelDispense {
    fn execute(&self, source: &mut BlockSource<'_>, mut dispensed: ItemStack) -> ItemStack {
        self.0.set_success(true);
        let facing = source.facing();
        let target_pos = source.pos.relative(facing);
        let level = &mut *source.level;
        let target = level.get_block_state(target_pos);
        if !try_ignite_explosive_entities(level, target_pos) {
            if base_fire_block::can_be_placed_at(level, target_pos, facing) {
                let fire = base_fire_block::get_state(&*level, target_pos);
                level.set_block_and_update(target_pos, fire);
                level.game_event_at_block(
                    GameEvent::BLOCK_PLACE,
                    target_pos,
                    &GameEventContext::of_entity(None),
                );
            } else if campfire_block::can_light(target)
                || candle_block::can_light(target)
                || candle_cake_block::can_light(target)
            {
                level.set_block_and_update(target_pos, target.set(props::LIT, true));
                level.game_event_at_block(
                    GameEvent::BLOCK_CHANGE,
                    target_pos,
                    &GameEventContext::of_entity(None),
                );
            } else if target.is_block(Block::TNT) {
                if tnt_block::prime(level, target_pos) {
                    level.remove_block(target_pos, false);
                } else {
                    self.0.set_success(false);
                }
            }
        }
        if self.0.is_success() {
            hurt_and_break_stack(&mut dispensed, 1, level, None);
        }
        dispensed
    }

    fn play_sound(&self, source: &mut BlockSource<'_>) {
        play_optional_sound(source, self.0.is_success());
    }
}

crate::register_dispense_behavior!(&["minecraft:flint_and_steel"], |_| Box::new(Templated(
    FlintAndSteelDispense(OptionalState::new())
)));
