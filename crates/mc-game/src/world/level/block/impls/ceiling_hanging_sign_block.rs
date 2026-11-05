//! `net.minecraft.world.level.block.CeilingHangingSignBlock` (extends `SignBlock`).
use super::prelude::*;
use super::sign_block;
use crate::server::level::ServerLevel;
use crate::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack};
use crate::world::level::block::rotation_segment;
use crate::world::level::shapes::is_face_full;

pub struct CeilingHangingSignBlock;

impl BlockBehaviour for CeilingHangingSignBlock {
    fn is_possible_to_respawn_in_this(&self, _state: BlockState) -> bool {
        true
    }
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        sign_block::use_without_item(level, pos, player)
    }
    fn set_placed_by(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, placer: Option<&mut crate::world::entity::ServerPlayer>, _stack: &ItemStack) {
        sign_block::set_placed_by(level, pos, placer)
    }
    /// `useItemOn`: a hanging sign clicked on its bottom face chains another hanging sign (`PASS`).
    fn use_item_on(
        &self,
        stack: &ItemStack,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        hand: InteractionHand,
        hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if sign_block::chains_hanging_sign(level, pos, player, stack, || hit.direction == Direction::Down) {
            InteractionResult::PASS
        } else {
            sign_block::use_item_on(stack, level, pos, player, hand)
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.above()).is_face_sturdy_support(level, pos.above(), Direction::Down, SupportType::Center)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let clicked = context.get_clicked_pos();
        let waterlogged = sign_block::waterlogged_by(&*level, clicked);
        let above = clicked.above();
        let state_above = level.get_block_state(above);
        let is_below_hanging_sign = state_above.is_tag("all_hanging_signs");
        let direction = Direction::from_y_rot(context.get_rotation() as f64);
        let secondary = context.is_secondary_use_active();
        let mut attached_to_middle = !is_face_full(&state_above.get_collision_shape(&*level, above), Direction::Down) || secondary;
        if is_below_hanging_sign && !secondary {
            if state_above.has_property(props::HORIZONTAL_FACING) {
                let above_direction = state_above.get(props::HORIZONTAL_FACING);
                if above_direction.get_axis().test(direction) {
                    attached_to_middle = false;
                }
            } else if state_above.has_property(props::ROTATION_16)
                && let Some(above_direction) = rotation_segment::convert_to_direction(state_above.get(props::ROTATION_16))
                && above_direction.get_axis().test(direction)
            {
                attached_to_middle = false;
            }
        }
        let rotation_segment = if !attached_to_middle {
            rotation_segment::convert_direction_to_segment(direction.get_opposite())
        } else {
            rotation_segment::convert_to_segment(context.get_rotation() + 180.0)
        };
        Some(
            block
                .default_state()
                .set(props::ATTACHED, attached_to_middle)
                .set(props::ROTATION_16, rotation_segment)
                .set(props::WATERLOGGED, waterlogged),
        )
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !self.can_survive(state, &*level, pos) { air() } else { sign_block::update_shape(state, level, pos) }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        state.set(props::ROTATION_16, rotation.rotate_steps(state.get(props::ROTATION_16), 16))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::ROTATION_16, mirror.mirror_steps(state.get(props::ROTATION_16), 16))
    }
}

register_block_class!("CeilingHangingSignBlock", |_| Box::new(CeilingHangingSignBlock));
