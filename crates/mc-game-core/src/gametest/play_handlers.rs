//! `ServerGamePacketListenerImpl.handleSetTestBlock` / `handleTestInstanceBlockAction`.

use crate::server::MinecraftServer;
use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};
use crate::world::block_entity::test_block_entity;
use crate::world::block_entity::test_instance_block_entity;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_data::block::Block;
use mc_data::enums::TestBlockMode;
use mc_protocol::packets::play::{
    ClientboundTestInstanceBlockStatus, ServerboundSetTestBlockPacket, ServerboundTestInstanceBlockActionPacket, ServerboundTestInstanceBlockActionPacketAction as Action,
    TestBlockMode as WireTestBlockMode,
};
use mc_text::{ChatFormatting, Component};

struct TestBlockHandlers;

impl PlayHandlers for TestBlockHandlers {
    fn handle_set_test_block(&self, ctx: &mut PlayCtx, packet: &ServerboundSetTestBlockPacket) {
        if !ctx.player.can_use_game_master_blocks() {
            return;
        }
        let pos = packet.position;
        let initial_state = ctx.level.get_block_state(pos);
        if test_block_entity::get(ctx.level, pos).is_none() {
            return;
        }
        let mode = match packet.mode {
            WireTestBlockMode::Start => TestBlockMode::Start,
            WireTestBlockMode::Log => TestBlockMode::Log,
            WireTestBlockMode::Fail => TestBlockMode::Fail,
            WireTestBlockMode::Accept => TestBlockMode::Accept,
        };
        test_block_entity::set_mode(ctx.level, pos, mode);
        if let Some(test_block) = test_block_entity::get(ctx.level, pos) {
            test_block.set_message(packet.message.clone());
        }
        ctx.level.block_entity_changed(pos);
        let state = ctx.level.get_block_state(pos);
        ctx.level.send_block_updated(pos, initial_state, state, 3);
    }

    fn handle_test_instance_block_action(&self, ctx: &mut PlayCtx, packet: &ServerboundTestInstanceBlockActionPacket) {
        let pos = packet.pos;
        let is_test_instance = ctx.level.get_block_entity(pos).is_some_and(|be| be.get_type() == mc_data::BlockEntityType::TEST_INSTANCE_BLOCK);
        if !ctx.player.can_use_game_master_blocks() || !is_test_instance {
            return;
        }
        let Some(handle) = ctx.player.handle.clone() else { return };
        if packet.action != Action::Query && packet.action != Action::Init {
            // the actions run once the player and its level are back in the server (they need other levels, the
            // registries and the ticker)
            let dimension = ctx.level.dimension.clone();
            let (action, data) = (packet.action, packet.data.clone());
            ctx.lis().deferred_tasks.push(Box::new(move |server: &mut MinecraftServer| {
                test_instance_block_entity::set(server, &dimension, pos, data);
                let mut feedback = |c: Component| handle.send_system_message(&c, false);
                match action {
                    Action::Reset => test_instance_block_entity::reset_test(server, &dimension, pos, &mut feedback),
                    Action::Save => {
                        test_instance_block_entity::save_test(server, &dimension, pos, &mut feedback);
                    }
                    Action::Export => {
                        test_instance_block_entity::export_test(server, &dimension, pos, &mut feedback);
                    }
                    Action::Run => test_instance_block_entity::run_test(server, &dimension, pos, &mut feedback),
                    _ => {}
                }
                if let Some(level) = server.level_mut(&dimension) {
                    let state = level.get_block_state(pos);
                    level.send_block_updated(pos, Block::AIR.default_state(), state, 3);
                }
            }));
        } else {
            let registries = ctx.server.registries.composite_access().clone();
            let test = packet.data.test.as_ref().and_then(|key| super::game_test_instance::lookup_test(&registries, key));
            let status = match &test {
                Some(test) => test.value.describe(),
                None => Component::translatable("test_instance.description.no_test", Vec::new()).with_format(ChatFormatting::Red),
            };
            let size = if packet.action == Action::Query {
                packet.data.test.as_ref().and_then(|key| test_instance_block_entity::get_structure_size_in(ctx.level, &registries, key))
            } else {
                None
            };
            ctx.send(&ClientboundTestInstanceBlockStatus { status, size });
        }
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static TEST_BLOCK_HANDLERS: &dyn PlayHandlers = &TestBlockHandlers;

