//! `net.minecraft.world.level.block.ShelfMushroomBlock` (extends `HorizontalDirectionalBlock`).
use mc_data::{EntityType, SoundEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

pub struct ShelfMushroomBlock;

impl BlockBehaviour for ShelfMushroomBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let facing = state.get(props::HORIZONTAL_FACING);
        let support_pos = pos.relative(facing.get_opposite());
        level
            .get_block_state(support_pos)
            .is_face_sturdy_at(level, support_pos, facing)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let mut state = block.default_state();
        let pos = context.get_clicked_pos();
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal() {
                state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
                if state.can_survive(&*level, pos) {
                    return Some(state);
                }
            }
        }
        None
    }
    fn bounce_on(
        &self,
        level: &mut ServerLevel,
        _state: BlockState,
        pos: BlockPos,
        entity: &mut dyn Entity,
        _fall_distance: f64,
    ) {
        if !matches!(entity.get_type(), EntityType::ITEM | EntityType::TNT) {
            level.play_sound(
                None,
                pos,
                SoundEvent::BLOCK_SHELF_MUSHROOM_BOUNCE,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(
        &self,
        _level: &dyn LevelReader,
        _pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) -> bool {
        state.get(props::AGE_1) < 1
    }
    fn perform_bonemeal(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _source: BonemealSource,
    ) {
        level.set_block_flags(pos, state.set(props::AGE_1, state.get(props::AGE_1) + 1), 2);
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == state.get(props::HORIZONTAL_FACING).get_opposite()
                && !state.can_survive(&*level, pos)
            {
                air()
            } else {
                state
            }
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("ShelfMushroomBlock", |_| Box::new(ShelfMushroomBlock));
