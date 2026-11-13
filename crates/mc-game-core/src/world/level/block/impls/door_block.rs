//! `net.minecraft.world.level.block.DoorBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::{EntityId, ServerPlayer};
use crate::world::item::{BlockPlaceContext, InteractionResult, ItemStack};
use crate::world::level::block::block_set_type::BlockSetType;
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::signal_getter::has_neighbor_signal;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_data::GameEvent;
use mc_data::block::PathComputationType;
use mc_data::enums::{DoorHingeSide, DoubleBlockHalf};
use mc_protocol::packets::play::level::SoundSource;

pub struct DoorBlock {
    block: Block,
    pub set_type: &'static BlockSetType,
}

impl DoorBlock {
    fn new(block: Block) -> Self {
        Self {
            block,
            set_type: BlockSetType::for_block(block).expect("door without BlockSetType"),
        }
    }

    /// `DoorBlock.playSound(entity, level, pos, open)`.
    fn play_sound(
        &self,
        entity: Option<EntityId>,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        open: bool,
    ) {
        let pitch = level.random().next_float() * 0.1 + 0.9;
        let sound = if open {
            self.set_type.door_open
        } else {
            self.set_type.door_close
        };
        level.play_sound(entity, pos, sound, SoundSource::Blocks, 1.0, pitch);
    }

    /// `DoorBlock.setOpen(sourceEntity, level, state, pos, shouldOpen)`.
    pub fn set_open(
        &self,
        source_entity: Option<EntityId>,
        level: &mut ServerLevel,
        state: BlockState,
        pos: BlockPos,
        should_open: bool,
    ) {
        if state.is_block(self.block) && state.get(props::OPEN) != should_open {
            level.set_block_flags(pos, state.set(props::OPEN, should_open), 10);
            self.play_sound(source_entity, level, pos, should_open);
            let event = if should_open {
                GameEvent::BLOCK_OPEN
            } else {
                GameEvent::BLOCK_CLOSE
            };
            level.game_event(
                event,
                Vec3::at_center_of(&pos),
                &GameEventContext::of(source_entity, None),
            );
        }
    }

    fn on_explosion_hit_inner(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        can_trigger_blocks: bool,
    ) {
        if can_trigger_blocks
            && state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
            && self.set_type.can_open_by_wind_charge
            && !state.get(props::POWERED)
        {
            self.set_open(None, level, state, pos, !state.get(props::OPEN));
        }
    }

    /// `DoorBlock.getHinge(context)`.
    fn get_hinge(level: &ServerLevel, context: &BlockPlaceContext) -> DoorHingeSide {
        let pos = context.get_clicked_pos();
        let place_direction = context.get_horizontal_direction();
        let above_pos = pos.above();
        let left_direction = place_direction.get_counter_clockwise();
        let left_pos = pos.relative(left_direction);
        let left_state = level.get_block_state(left_pos);
        let left_above_pos = above_pos.relative(left_direction);
        let left_above_state = level.get_block_state(left_above_pos);
        let right_direction = place_direction.get_clockwise();
        let right_pos = pos.relative(right_direction);
        let right_state = level.get_block_state(right_pos);
        let right_above_pos = above_pos.relative(right_direction);
        let right_above_state = level.get_block_state(right_above_pos);
        let solid_block_balance =
            (if left_state.is_collision_shape_full_block_at(level, left_pos) {
                -1
            } else {
                0
            }) + (if left_above_state.is_collision_shape_full_block_at(level, left_above_pos) {
                -1
            } else {
                0
            }) + (if right_state.is_collision_shape_full_block_at(level, right_pos) {
                1
            } else {
                0
            }) + (if right_above_state.is_collision_shape_full_block_at(level, right_above_pos) {
                1
            } else {
                0
            });
        let door_left = helpers::is_instance_of(left_state, "DoorBlock")
            && left_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower;
        let door_right = helpers::is_instance_of(right_state, "DoorBlock")
            && right_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower;
        if (!door_left || door_right) && solid_block_balance <= 0 {
            if (!door_right || door_left) && solid_block_balance >= 0 {
                let step_x = place_direction.get_step_x();
                let step_z = place_direction.get_step_z();
                let click_location = context.use_on.get_click_location();
                let click_x = click_location.x - pos.x() as f64;
                let click_z = click_location.z - pos.z() as f64;
                if (step_x >= 0 || click_z >= 0.5)
                    && (step_x <= 0 || click_z <= 0.5)
                    && (step_z >= 0 || click_x <= 0.5)
                    && (step_z <= 0 || click_x >= 0.5)
                {
                    DoorHingeSide::Left
                } else {
                    DoorHingeSide::Right
                }
            } else {
                DoorHingeSide::Left
            }
        } else {
            DoorHingeSide::Right
        }
    }
}

/// `DoublePlantBlock.preventDropFromBottomPart(level, pos, state, player)`.
pub fn prevent_drop_from_bottom_part(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    player: &ServerPlayer,
) {
    if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Upper {
        let bottom_pos = pos.below();
        let bottom_state = level.get_block_state(bottom_pos);
        if bottom_state.is_block(state.block())
            && bottom_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
        {
            let replacement = if bottom_state.get_fluid_state().is_tag("water") {
                mc_data::block::Block::WATER.default_state()
            } else {
                air()
            };
            level.set_block_flags(bottom_pos, replacement, 35);
            level.level_event(
                Some(player.base_id()),
                2001,
                bottom_pos,
                bottom_state.id() as i32,
            );
        }
    }
}

/// `((DoorBlock) state.getBlock()).setOpen(source, level, state, pos, open)`.
pub fn set_open_at(
    source: Option<EntityId>,
    level: &mut ServerLevel,
    state: BlockState,
    pos: BlockPos,
    open: bool,
) {
    DoorBlock::new(state.block()).set_open(source, level, state, pos, open);
}

/// `DoorBlock.isWoodenDoor(state)`.
pub fn is_wooden_door(state: BlockState) -> bool {
    helpers::is_instance_of(state, "DoorBlock")
        && BlockSetType::for_block(state.block()).is_some_and(|t| t.can_open_by_hand)
}

impl BlockBehaviour for DoorBlock {
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn crate::world::level::explosion::Explosion,
        on_hit: &mut dyn FnMut(ItemStack, BlockPos),
    ) {
        self.on_explosion_hit_inner(state, level, pos, explosion.can_trigger_blocks());
        crate::world::level::explosion::default_on_explosion_hit(
            state, level, pos, explosion, on_hit,
        );
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = pos.below();
        let below_state = level.get_block_state(below);
        if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower {
            below_state.is_face_sturdy_at(level, below, Direction::Up)
        } else {
            below_state.is_block(state.block())
        }
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            let half = state.get(props::DOUBLE_BLOCK_HALF);
            if direction.get_axis() != Axis::Y
                || (half == DoubleBlockHalf::Lower) != (direction == Direction::Up)
            {
                if half == DoubleBlockHalf::Lower
                    && direction == Direction::Down
                    && !state.can_survive(&*level, pos)
                {
                    air()
                } else {
                    state
                }
            } else if helpers::is_instance_of(neighbour_state, "DoorBlock")
                && neighbour_state.get(props::DOUBLE_BLOCK_HALF) != half
            {
                neighbour_state.set(props::DOUBLE_BLOCK_HALF, half)
            } else {
                air()
            }
        }
    );
    fn player_will_destroy(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
    ) -> BlockState {
        if player.player.abilities.instabuild || !player.has_correct_tool_for_drops(state) {
            prevent_drop_from_bottom_part(level, pos, state, player);
        }
        crate::server::level::block_break::player_will_destroy(level, pos, state, player)
    }
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        match ty {
            PathComputationType::Land | PathComputationType::Air => state.get(props::OPEN),
            PathComputationType::Water => false,
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        if pos.y() < level.get_max_y()
            && level
                .get_block_state(pos.above())
                .can_be_replaced_by(context)
        {
            let powered =
                has_neighbor_signal(&*level, pos) || has_neighbor_signal(&*level, pos.above());
            Some(
                block
                    .default_state()
                    .set(props::HORIZONTAL_FACING, context.get_horizontal_direction())
                    .set(props::DOOR_HINGE, Self::get_hinge(level, context))
                    .set(props::POWERED, powered)
                    .set(props::OPEN, powered)
                    .set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Lower),
            )
        } else {
            None
        }
    }
    fn set_placed_by(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _placer: Option<&mut ServerPlayer>,
        _stack: &ItemStack,
    ) {
        level.set_block_and_update(
            pos.above(),
            state.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper),
        );
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if !self.set_type.can_open_by_hand {
            return InteractionResult::PASS;
        }
        let state = state.cycle(props::OPEN);
        level.set_block_flags(pos, state, 10);
        let open = state.get(props::OPEN);
        self.play_sound(Some(player.base_id()), level, pos, open);
        let event = if open {
            GameEvent::BLOCK_OPEN
        } else {
            GameEvent::BLOCK_CLOSE
        };
        level.game_event_from_entity_ref_in_state(&*player, None, event, Vec3::at_center_of(&pos));
        InteractionResult::SUCCESS
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        block: Block,
        _orientation: Option<crate::world::level::redstone::Orientation>,
        _moved_by_piston: bool,
    ) {
        let other = if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower {
            Direction::Up
        } else {
            Direction::Down
        };
        let signal =
            has_neighbor_signal(&*level, pos) || has_neighbor_signal(&*level, pos.relative(other));
        if state.block() != block && signal != state.get(props::POWERED) {
            if signal != state.get(props::OPEN) {
                self.play_sound(None, level, pos, signal);
                let event = if signal {
                    GameEvent::BLOCK_OPEN
                } else {
                    GameEvent::BLOCK_CLOSE
                };
                level.game_event(
                    event,
                    Vec3::at_center_of(&pos),
                    &GameEventContext::of(None, None),
                );
            }
            level.set_block_flags(
                pos,
                state.set(props::POWERED, signal).set(props::OPEN, signal),
                2,
            );
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        if mirror == Mirror::None {
            state
        } else {
            state
                .rotate(mirror.get_rotation(state.get(props::HORIZONTAL_FACING)))
                .cycle(props::DOOR_HINGE)
        }
    }
    fn get_seed(&self, state: BlockState, pos: BlockPos) -> i64 {
        let y = pos.y()
            - if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower {
                0
            } else {
                1
            };
        mc_core::math::mth::get_seed_xyz(pos.x(), y, pos.z())
    }
}

register_block_class!("DoorBlock", |block| Box::new(DoorBlock::new(block)));
