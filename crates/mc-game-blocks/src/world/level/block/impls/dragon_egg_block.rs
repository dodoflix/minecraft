//! `net.minecraft.world.level.block.DragonEggBlock` (extends `FallingBlock`, `getDelayAfterPlace` = 5).
use mc_core::random::RandomSource;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::InteractionResult;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::impls::falling_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct DragonEggBlock;

const HORIZONTAL_TELEPORT_RADIUS: i32 = 16;
const VERTICAL_TELEPORT_RADIUS: i32 = 8;

/// `DragonEggBlock.teleport(state, level, pos)`: moves the egg to a random air block with a solid-ish block below (1000 tries).
fn teleport(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    for _ in 0..1000 {
        let random = level.random();
        let dx = random.next_int_bounded(HORIZONTAL_TELEPORT_RADIUS)
            - random.next_int_bounded(HORIZONTAL_TELEPORT_RADIUS);
        let dy = random.next_int_bounded(VERTICAL_TELEPORT_RADIUS)
            - random.next_int_bounded(VERTICAL_TELEPORT_RADIUS);
        let dz = random.next_int_bounded(HORIZONTAL_TELEPORT_RADIUS)
            - random.next_int_bounded(HORIZONTAL_TELEPORT_RADIUS);
        let test_pos = pos.offset(dx, dy, dz);
        if level.get_block_state(test_pos).is_air()
            && !level.get_block_state(test_pos.below()).is_air()
            && level.world_border().is_within_bounds_block(&test_pos)
            && !level.is_outside_build_height(test_pos.y())
        {
            // `BlockUtil.packDifferenceInPosition(pos, testPos, 16, 8, 16)`
            let packed_diff = ((dx + HORIZONTAL_TELEPORT_RADIUS) & 0xFF) << 16
                | ((dy + VERTICAL_TELEPORT_RADIUS) & 0xFF) << 8
                | ((dz + HORIZONTAL_TELEPORT_RADIUS) & 0xFF);
            LevelAccessor::level_event(level, None, 2015, pos, packed_diff);
            level.set_block_flags(test_pos, state, 2);
            level.remove_block(pos, false);
            return;
        }
    }
}

impl BlockBehaviour for DragonEggBlock {
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        falling_block::on_place(state, level, pos, 5);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        falling_block::tick(state, level, pos);
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            falling_block::update_shape(state, level, pos, 5)
        }
    );
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _player: &mut ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        teleport(state, level, pos);
        InteractionResult::SUCCESS
    }
    fn attack(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _player: &mut ServerPlayer,
    ) {
        teleport(state, level, pos);
    }
}

register_block_class!("DragonEggBlock", |_| Box::new(DragonEggBlock));
