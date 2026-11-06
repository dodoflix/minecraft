//! `net.minecraft.world.level.block.CommandBlock`: redstone / auto / chain execution, the chain walk, the editor
//! hand-off and the comparator output.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::CommandBlockEntity;
use mc_game_core::world::item::{BlockPlaceContext, InteractionResult};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::has_neighbor_signal;
use mc_protocol::packets::play::CommandBlockEntityMode as Mode;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

pub struct CommandBlock {
    block: Block,
    /// `automatic`: the block entity is created with `auto` set (chain command blocks).
    automatic: bool,
}

impl CommandBlock {
    /// `setPoweredAndUpdate(level, pos, commandBlock, isPowered)`.
    fn set_powered_and_update(&self, level: &mut ServerLevel, pos: BlockPos, is_powered: bool) {
        level.with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
            let was_powered = be.is_powered();
            if is_powered != was_powered {
                be.set_powered(is_powered);
                if is_powered {
                    if be.is_automatic() || be.get_mode() == Mode::Sequence {
                        return;
                    }
                    be.mark_condition_met(level);
                    level.schedule_tick_block(pos, self.block, 1);
                }
            }
        });
    }

    /// `execute(state, level, pos, baseCommandBlock, commandSet)`.
    fn execute(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        command_set: bool,
    ) {
        if command_set {
            CommandBlockEntity::perform_command(level, pos);
        } else {
            level.with_typed_block_entity::<CommandBlockEntity, _>(pos, |_, be| {
                be.get_command_block_mut().set_success_count(0)
            });
        }
        execute_chain(level, pos, state.get(props::FACING));
    }
}

/// `baseCommandBlock.setSuccessCount(0)` when the block is conditional (`else if (commandBlock.isConditional())`).
fn reset_success_count_if_conditional(level: &mut ServerLevel, pos: BlockPos) {
    level.with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
        if be.is_conditional(level) {
            be.get_command_block_mut().set_success_count(0);
        }
    });
}

/// `CommandBlock.executeChain(level, blockPos, direction)`.
fn execute_chain(level: &mut ServerLevel, block_pos: BlockPos, mut direction: Direction) {
    let mut pos = block_pos;
    let rule = level.command_rules().max_command_sequence_length;
    let mut max_iterations = rule;
    // `while (maxIterations-- > 0)`
    while {
        let go = max_iterations > 0;
        max_iterations = max_iterations.wrapping_sub(1);
        go
    } {
        pos = pos.relative(direction);
        let state = level.get_block_state(pos);
        let block = state.block();
        if block != Block::CHAIN_COMMAND_BLOCK {
            break;
        }
        // `(level.getBlockEntity(pos) instanceof CommandBlockEntity) && mode == SEQUENCE`, then `isPowered() || isAutomatic()`
        let Some(Some(active)) = level
            .with_typed_block_entity::<CommandBlockEntity, _>(pos, |_, be| {
                (be.get_mode() == Mode::Sequence).then(|| be.is_powered() || be.is_automatic())
            })
        else {
            break;
        };
        if active {
            let condition_met = level
                .with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
                    be.mark_condition_met(level)
                })
                .unwrap_or(false);
            if condition_met {
                if !CommandBlockEntity::perform_command(level, pos) {
                    break;
                }
                level.update_neighbour_for_output_signal(pos, block);
            } else {
                reset_success_count_if_conditional(level, pos);
            }
        }
        direction = state.get(props::FACING);
    }
    if max_iterations <= 0 {
        log::warn!(
            "Command Block chain tried to execute more than {} steps!",
            rule.max(0)
        );
    }
}

impl BlockBehaviour for CommandBlock {
    fn neighbor_changed(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        if level
            .get_block_entity_immediate(pos)
            .is_some_and(|be| be.as_any().is::<CommandBlockEntity>())
        {
            let powered = has_neighbor_signal(&*level, pos);
            self.set_powered_and_update(level, pos, powered);
        }
    }

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let Some((command_set, mode, was_condition_met)) = level
            .with_typed_block_entity::<CommandBlockEntity, _>(pos, |_, be| {
                (
                    !be.get_command_block().get_command().is_empty(),
                    be.get_mode(),
                    be.was_condition_met(),
                )
            })
        else {
            return;
        };
        if mode == Mode::Auto {
            level.with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
                be.mark_condition_met(level)
            });
            if was_condition_met {
                self.execute(state, level, pos, command_set);
            } else {
                reset_success_count_if_conditional(level, pos);
            }
            let keep_ticking = level
                .with_typed_block_entity::<CommandBlockEntity, _>(pos, |_, be| {
                    be.is_powered() || be.is_automatic()
                })
                .unwrap_or(false);
            if keep_ticking {
                level.schedule_tick_block(pos, self.block, 1);
            }
        } else if mode == Mode::Redstone {
            if was_condition_met {
                self.execute(state, level, pos, command_set);
            } else {
                reset_success_count_if_conditional(level, pos);
            }
        }
        level.update_neighbour_for_output_signal(pos, self.block);
    }

    /// `useWithoutItem`: `player.openCommandBlock(be)` (server side: the entity's custom-only data to the client).
    fn use_without_item(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        let Some(be) = level.get_block_entity_immediate(pos) else {
            return InteractionResult::PASS;
        };
        if !be.as_any().is::<CommandBlockEntity>() || !player.can_use_game_master_blocks() {
            return InteractionResult::PASS;
        }
        let mut tag = mc_nbt::CompoundTag::new();
        be.save_additional(&mut tag);
        if let Some(listener) = &player.listener {
            listener.send(&ClientboundBlockEntityDataPacket {
                pos,
                r#type: be.get_type(),
                tag,
            });
        }
        InteractionResult::SUCCESS
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<CommandBlockEntity>())
            .map_or(0, |be| be.get_command_block().get_success_count())
    }

    fn set_placed_by(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _placer: Option<&mut mc_game_core::world::entity::ServerPlayer>,
        stack: &mc_game_core::world::item::ItemStack,
    ) {
        if !level
            .get_block_entity_immediate(pos)
            .is_some_and(|be| be.as_any().is::<CommandBlockEntity>())
        {
            return;
        }
        if stack
            .get(&mc_protocol::components::BLOCK_ENTITY_DATA)
            .is_none()
        {
            let send_feedback = level.command_rules().send_command_feedback;
            level.with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
                be.get_command_block_mut().set_track_output(send_feedback);
                be.set_automatic(Some(level), self.automatic);
            });
        }
        let has_neighbor_signal = has_neighbor_signal(&*level, pos);
        self.set_powered_and_update(level, pos, has_neighbor_signal);
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }

    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }

    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::FACING,
            context.get_nearest_looking_direction().get_opposite(),
        ))
    }
}

register_block_class!("CommandBlock", |block| Box::new(CommandBlock {
    block,
    automatic: block == Block::CHAIN_COMMAND_BLOCK
}));
