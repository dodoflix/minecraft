//! `net.minecraft.world.level.block.WallHangingSignBlock` (extends `SignBlock`).
use super::prelude::*;
use super::sign_block;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack};
use mc_data::block::PathComputationType;

pub struct WallHangingSignBlock;

/// `WallHangingSignBlock.canAttachTo(level, state, attachPos, attachFace)`.
fn can_attach_to(
    level: &dyn LevelReader,
    state: BlockState,
    attach_pos: BlockPos,
    attach_face: Direction,
) -> bool {
    let attach_state = level.get_block_state(attach_pos);
    if attach_state.is_tag("wall_hanging_signs") {
        attach_state
            .get(props::HORIZONTAL_FACING)
            .get_axis()
            .test(state.get(props::HORIZONTAL_FACING))
    } else {
        attach_state.is_face_sturdy_support(level, attach_pos, attach_face, SupportType::Full)
    }
}

/// `WallHangingSignBlock.canPlace(state, level, pos)`: the sign can hang between two sturdy sides / signs next to it.
pub fn can_place(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    let facing = state.get(props::HORIZONTAL_FACING);
    let clockwise = facing.get_clockwise();
    let counter_clockwise = facing.get_counter_clockwise();
    can_attach_to(level, state, pos.relative(clockwise), counter_clockwise)
        || can_attach_to(level, state, pos.relative(counter_clockwise), clockwise)
}

impl BlockBehaviour for WallHangingSignBlock {
    crate::simple_waterlogged!();
    fn is_possible_to_respawn_in_this(&self, _state: BlockState) -> bool {
        true
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        sign_block::use_without_item(level, pos, player)
    }
    fn set_placed_by(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        placer: Option<&mut ServerPlayer>,
        _stack: &ItemStack,
    ) {
        sign_block::set_placed_by(level, pos, placer)
    }
    /// `useItemOn`: a hanging sign clicked on a side that is not an editable face chains another hanging sign (`PASS`).
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        let editable_side =
            hit.direction.get_axis() == state.get(props::HORIZONTAL_FACING).get_axis();
        if sign_block::chains_hanging_sign(level, pos, player, stack, || !editable_side) {
            InteractionResult::PASS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let mut state = block.default_state();
        let pos = context.get_clicked_pos();
        let waterlogged = sign_block::waterlogged_by(&*level, pos);
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal()
                && !direction.get_axis().test(context.get_clicked_face())
            {
                state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
                if state.can_survive(&*level, pos) && can_place(state, &*level, pos) {
                    return Some(state.set(props::WATERLOGGED, waterlogged));
                }
            }
        }
        None
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction.get_axis()
                == state
                    .get(props::HORIZONTAL_FACING)
                    .get_clockwise()
                    .get_axis()
                && !state.can_survive(&*level, pos)
            {
                air()
            } else {
                sign_block::update_shape(state, level, pos)
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

register_block_class!("WallHangingSignBlock", |_| Box::new(WallHangingSignBlock));
