//! `net.minecraft.world.level.block.FenceGateBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{BlockPlaceContext, InteractionResult};
use crate::world::level::block::block_set_type::WoodType;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::redstone::Orientation;
use crate::world::level::signal_getter::has_neighbor_signal;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::random::RandomSource;
use mc_data::block::PathComputationType;
use mc_data::GameEvent;
use mc_protocol::packets::play::level::SoundSource;

pub struct FenceGateBlock {
    wood: WoodType,
}

/// `FenceGateBlock.connectsToDirection`.
pub fn connects_to_direction(state: BlockState, direction: Direction) -> bool {
    state.get(props::HORIZONTAL_FACING).get_axis() == direction.get_clockwise().get_axis()
}

fn is_wall(state: BlockState) -> bool {
    state.is_tag("walls")
}

impl FenceGateBlock {
    fn play_sound(&self, level: &mut ServerLevel, except: Option<crate::world::entity::EntityId>, pos: BlockPos, open: bool) {
        let pitch = level.random().next_float() * 0.1 + 0.9;
        let sound = if open { self.wood.fence_gate_open } else { self.wood.fence_gate_close };
        level.play_sound(except, pos, sound, SoundSource::Blocks, 1.0, pitch);
    }

    /// `FenceGateBlock.onExplosionHit` (`canTriggerBlocks` = `explosion.canTriggerBlocks()`).
    pub fn on_explosion_hit(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, can_trigger_blocks: bool) {
        if can_trigger_blocks && !state.get(props::POWERED) {
            let open = state.get(props::OPEN);
            level.set_block_and_update(pos, state.set(props::OPEN, !open));
            self.play_sound(level, None, pos, !open);
            let event = if open { GameEvent::BLOCK_CLOSE } else { GameEvent::BLOCK_OPEN };
            level.game_event(event, Vec3::at_center_of(&pos), &GameEventContext::of_state(Some(state)));
        }
    }
}

impl BlockBehaviour for FenceGateBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let axis = direction.get_axis();
        if state.get(props::HORIZONTAL_FACING).get_clockwise().get_axis() != axis {
            state
        } else {
            let in_wall = is_wall(neighbour_state) || is_wall(level.get_block_state(pos.relative(direction.get_opposite())));
            state.set(props::IN_WALL, in_wall)
        }
    });
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        match ty {
            PathComputationType::Land | PathComputationType::Air => state.get(props::OPEN),
            PathComputationType::Water => false,
        }
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let is_open = has_neighbor_signal(&*level, pos);
        let direction = context.get_horizontal_direction();
        let axis = direction.get_axis();
        let in_wall = (axis == Axis::Z && (is_wall(level.get_block_state(pos.west())) || is_wall(level.get_block_state(pos.east()))))
            || (axis == Axis::X && (is_wall(level.get_block_state(pos.north())) || is_wall(level.get_block_state(pos.south()))));
        Some(block.default_state().set(props::HORIZONTAL_FACING, direction).set(props::OPEN, is_open).set(props::POWERED, is_open).set(props::IN_WALL, in_wall))
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &BlockHitResult) -> InteractionResult {
        let mut state = state;
        if state.get(props::OPEN) {
            state = state.set(props::OPEN, false);
            level.set_block_flags(pos, state, 10);
        } else {
            let direction = Direction::from_y_rot(player.base().y_rot() as f64);
            if state.get(props::HORIZONTAL_FACING) == direction.get_opposite() {
                state = state.set(props::HORIZONTAL_FACING, direction);
            }
            state = state.set(props::OPEN, true);
            level.set_block_flags(pos, state, 10);
        }
        let opens = state.get(props::OPEN);
        let id = Some(player.base_id());
        self.play_sound(level, id, pos, opens);
        let event = if opens { GameEvent::BLOCK_OPEN } else { GameEvent::BLOCK_CLOSE };
        level.game_event(event, Vec3::at_center_of(&pos), &GameEventContext::of(id, None));
        InteractionResult::SUCCESS
    }
    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        let has_power = has_neighbor_signal(&*level, pos);
        if state.get(props::POWERED) != has_power {
            level.set_block_flags(pos, state.set(props::POWERED, has_power).set(props::OPEN, has_power), 2);
            if state.get(props::OPEN) != has_power {
                self.play_sound(level, None, pos, has_power);
                let event = if has_power { GameEvent::BLOCK_OPEN } else { GameEvent::BLOCK_CLOSE };
                level.game_event(event, Vec3::at_center_of(&pos), &GameEventContext::of(None, None));
            }
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("FenceGateBlock", |block| Box::new(FenceGateBlock { wood: WoodType::for_fence_gate(block).expect("fence gate without WoodType") }));
