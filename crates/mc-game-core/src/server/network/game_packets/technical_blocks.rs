//! The `ServerGamePacketListenerImpl` handlers of the technical blocks' editor screens: `handleSetCommandBlock`,
//! `handleSetCommandMinecart`, `handleSetStructureBlock`, `handleSetJigsawBlock`. (`handleJigsawGenerate` needs the structure
//! placement and lives in `mc-game-levelgen`; `handleSetTestBlock` / `handleTestInstanceBlockAction` belong to the test blocks.)

use super::PlayCtx;
use super::hooks::{PLAY_HANDLERS, PlayHandlers};
use crate::translatable;
use crate::world::block_entity::command_block_entity::CommandBlockEntity;
use crate::world::block_entity::{BlockEntity, JigsawBlockEntity, StructureBlockEntity};
use crate::world::entity::vehicle::minecart::minecart_command_block::MinecartCommandBlock;
use crate::world::level::gamerules::GameRules;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::props;
use mc_protocol::packets::play::{
    CommandBlockEntityMode, ServerboundSetCommandBlockPacket, ServerboundSetCommandMinecartPacket,
    ServerboundSetJigsawBlockPacket, ServerboundSetStructureBlockPacket,
    StructureBlockEntityUpdateType, StructureMode as PacketStructureMode,
};

struct TechnicalBlockHandlers;

fn command_block_state(mode: CommandBlockEntityMode) -> mc_data::block::BlockState {
    match mode {
        CommandBlockEntityMode::Sequence => Block::CHAIN_COMMAND_BLOCK,
        CommandBlockEntityMode::Auto => Block::REPEATING_COMMAND_BLOCK,
        CommandBlockEntityMode::Redstone => Block::COMMAND_BLOCK,
    }
    .default_state()
}

fn structure_mode(mode: PacketStructureMode) -> mc_data::enums::StructureMode {
    match mode {
        PacketStructureMode::Save => mc_data::enums::StructureMode::Save,
        PacketStructureMode::Load => mc_data::enums::StructureMode::Load,
        PacketStructureMode::Corner => mc_data::enums::StructureMode::Corner,
        PacketStructureMode::Data => mc_data::enums::StructureMode::Data,
    }
}

impl PlayHandlers for TechnicalBlockHandlers {
    fn handle_set_command_block(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCommandBlockPacket,
    ) {
        if !ctx.player.can_use_game_master_blocks() {
            ctx.player
                .send_system_message(translatable!("advMode.notAllowed"));
            return;
        }
        let pos: BlockPos = packet.pos;
        let Some(old_mode) = ctx
            .level
            .with_typed_block_entity::<CommandBlockEntity, _>(pos, |_, be| be.get_mode())
        else {
            return;
        };
        let command = packet.command.as_str();
        let track_output = packet.track_output;
        let current_state = ctx.level.get_block_state(pos);
        let direction = current_state.get(props::FACING);
        let state = command_block_state(packet.mode)
            .set(props::FACING, direction)
            .set(props::CONDITIONAL, packet.conditional);
        if state != current_state {
            // vanilla keeps the very entity object across the block change (`setBlockState` + `chunk.setBlockEntity`)
            let old = if state.block() != current_state.block() {
                ctx.level.take_block_entity(pos)
            } else {
                None
            };
            ctx.level.set_block(pos, state, 2, 512);
            if let Some(mut block_entity) = old {
                block_entity.set_block_state(state);
                ctx.level.add_and_register_block_entity(block_entity);
            }
        }
        CommandBlockEntity::set_command(ctx.level, pos, command);
        let automatic = packet.automatic;
        let new_mode = packet.mode;
        ctx.level
            .with_typed_block_entity::<CommandBlockEntity, _>(pos, |level, be| {
                let base = be.get_command_block_mut();
                base.set_track_output(track_output);
                if !track_output {
                    base.set_last_output(None);
                }
                be.set_automatic(Some(&mut *level), automatic);
                if old_mode != new_mode {
                    be.on_mode_switch(level);
                }
            });
        let enabled = ctx
            .server
            .game_rules_ref()
            .get(GameRules::COMMAND_BLOCKS_WORK);
        if enabled {
            CommandBlockEntity::on_updated(ctx.level, pos);
        }
        if !command.is_empty() {
            ctx.player.send_system_message(translatable!(
                if enabled {
                    "advMode.setCommand.success"
                } else {
                    "advMode.setCommand.disabled"
                },
                command
            ));
        }
    }

    fn handle_set_command_minecart(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetCommandMinecartPacket,
    ) {
        if !ctx.player.can_use_game_master_blocks() {
            ctx.player
                .send_system_message(translatable!("advMode.notAllowed"));
            return;
        }
        let command = packet.command.as_str();
        let enabled = ctx
            .server
            .game_rules_ref()
            .get(GameRules::COMMAND_BLOCKS_WORK);
        let track_output = packet.track_output;
        let found = ctx
            .level
            .with_entity(packet.entity, |_, entity| {
                let cart = entity.as_any_mut().downcast_mut::<MinecartCommandBlock>()?;
                let base = cart.get_command_block_mut();
                base.set_command(command);
                base.set_track_output(track_output);
                if !track_output {
                    base.set_last_output(None);
                }
                if enabled {
                    cart.on_updated();
                }
                Some(())
            })
            .flatten();
        if found.is_some() && !command.is_empty() {
            ctx.player.send_system_message(translatable!(
                if enabled {
                    "advMode.setCommand.success"
                } else {
                    "advMode.setCommand.disabled"
                },
                command
            ));
        }
    }

    fn handle_set_structure_block(
        &self,
        ctx: &mut PlayCtx,
        packet: &ServerboundSetStructureBlockPacket,
    ) {
        if !ctx.player.can_use_game_master_blocks() {
            return;
        }
        let pos = packet.pos;
        let state = ctx.level.get_block_state(pos);
        if !ctx
            .level
            .get_block_entity_immediate(pos)
            .is_some_and(|be| be.as_any().is::<StructureBlockEntity>())
        {
            return;
        }
        StructureBlockEntity::set_mode(ctx.level, pos, structure_mode(packet.mode));
        let (has_name, actual_name) = ctx
            .level
            .with_typed_block_entity::<StructureBlockEntity, _>(pos, |_, structure| {
                structure.set_structure_name(&packet.name);
                structure.set_structure_pos(packet.offset);
                structure.set_structure_size(packet.size);
                structure.set_mirror(packet.mirror);
                structure.set_rotation(packet.rotation);
                structure.set_meta_data(&packet.data);
                structure.set_ignore_entities(packet.ignore_entities);
                structure.set_strict(packet.strict);
                structure.set_show_air(packet.show_air);
                structure.set_show_bounding_box(packet.show_bounding_box);
                structure.set_integrity(packet.integrity);
                structure.set_seed(packet.seed);
                (
                    structure.has_structure_name(),
                    structure.get_structure_name(),
                )
            })
            .expect("structure block entity checked above");
        if has_name {
            match packet.update_type {
                StructureBlockEntityUpdateType::SaveArea => {
                    let saved = StructureBlockEntity::save_structure_in_save_mode(ctx.level, pos);
                    ctx.player.send_system_message(translatable!(
                        if saved {
                            "structure_block.save_success"
                        } else {
                            "structure_block.save_failure"
                        },
                        actual_name
                    ));
                }
                StructureBlockEntityUpdateType::LoadArea => {
                    if !StructureBlockEntity::is_structure_loadable(ctx.level, pos) {
                        ctx.player.send_system_message(translatable!(
                            "structure_block.load_not_found",
                            actual_name
                        ));
                    } else if StructureBlockEntity::place_structure_if_same_size(ctx.level, pos) {
                        ctx.player.send_system_message(translatable!(
                            "structure_block.load_success",
                            actual_name
                        ));
                    } else {
                        ctx.player.send_system_message(translatable!(
                            "structure_block.load_prepare",
                            actual_name
                        ));
                    }
                }
                StructureBlockEntityUpdateType::ScanArea => {
                    if StructureBlockEntity::detect_size(ctx.level, pos) {
                        ctx.player.send_system_message(translatable!(
                            "structure_block.size_success",
                            actual_name
                        ));
                    } else {
                        ctx.player
                            .send_system_message(translatable!("structure_block.size_failure"));
                    }
                }
                StructureBlockEntityUpdateType::UpdateData => {}
            }
        } else {
            ctx.player.send_system_message(translatable!(
                "structure_block.invalid_structure_name",
                packet.name.clone()
            ));
        }
        ctx.level
            .with_typed_block_entity::<StructureBlockEntity, _>(pos, |level, structure| {
                level.block_entity_set_changed(pos, structure.get_block_state())
            });
        ctx.level.send_block_updated(pos, state, state, 3);
    }

    fn handle_set_jigsaw_block(&self, ctx: &mut PlayCtx, packet: &ServerboundSetJigsawBlockPacket) {
        if !ctx.player.can_use_game_master_blocks() {
            return;
        }
        let pos = packet.pos;
        let state = ctx.level.get_block_state(pos);
        let updated = ctx.level.with_typed_block_entity::<JigsawBlockEntity, _>(pos, |level, jigsaw| {
            jigsaw.set_name(packet.name.clone());
            jigsaw.set_target(packet.target.clone());
            jigsaw.set_pool(packet.pool.clone());
            jigsaw.set_final_state(&packet.final_state);
            jigsaw.set_joint(match packet.joint {
                mc_protocol::packets::play::JigsawBlockEntityJointType::Rollable => crate::world::level::levelgen::structure::templatesystem::structure_template::JointType::Rollable,
                mc_protocol::packets::play::JigsawBlockEntityJointType::Aligned => crate::world::level::levelgen::structure::templatesystem::structure_template::JointType::Aligned,
            });
            jigsaw.set_placement_priority(packet.placement_priority);
            jigsaw.set_selection_priority(packet.selection_priority);
            level.block_entity_set_changed(pos, jigsaw.get_block_state());
        });
        if updated.is_some() {
            ctx.level.send_block_updated(pos, state, state, 3);
        }
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static HANDLERS: &dyn PlayHandlers = &TechnicalBlockHandlers;
