//! `ServerGamePacketListenerImpl.handleJigsawGenerate` and `JigsawBlockEntity.generate`: the "Generate" button of the jigsaw
//! block screen places the pieces of the block's pool straight into the level.

use crate::world::level::levelgen::structure::pools::jigsaw_placement::generate_jigsaw;
use linkme::distributed_slice;
use mc_core::direction::FrontAndTop;
use mc_core::pos::BlockPos;
use mc_data::props;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::server::level::server_level_worldgen::with_worldgen_clock;
use mc_game_core::server::network::game_packets::PlayCtx;
use mc_game_core::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};
use mc_game_core::world::block_entity::JigsawBlockEntity;
use mc_game_core::world::level::BlockGetter;
use mc_protocol::packets::play::ServerboundJigsawGeneratePacket;

/// `JigsawBlockEntity.generate(level, levels, keepJigsaws)` for the jigsaw block entity at `pos`. A pool the registry does
/// not know is vanilla's `getOrThrow` exception, which the packet handler logs and suppresses.
pub fn generate(level: &mut ServerLevel, pos: BlockPos, levels: i32, keep_jigsaws: bool) {
    let Some((pool_id, target)) = level
        .get_block_entity(pos)
        .and_then(|be| be.as_any().downcast_ref::<JigsawBlockEntity>())
        .map(|jigsaw| (jigsaw.get_pool().clone(), jigsaw.get_target().clone()))
    else {
        return;
    };
    let front =
        FrontAndTop::ALL[level.get_block_state(pos).get(props::ORIENTATION).ordinal()].front();
    let position = pos.relative(front);
    let Some(pool) = level
        .gen_env()
        .structures
        .template_pools
        .get(&pool_id)
        .cloned()
    else {
        log::error!("Failed to generate jigsaw at {pos:?}: unknown template pool {pool_id}");
        return;
    };
    let mut random = level.tick_state.random.clone();
    generate_jigsaw(
        level,
        &mut random,
        &pool,
        &target,
        levels,
        position,
        keep_jigsaws,
    );
}

struct JigsawGenerateHandler;

impl PlayHandlers for JigsawGenerateHandler {
    fn handle_jigsaw_generate(&self, ctx: &mut PlayCtx, packet: &ServerboundJigsawGeneratePacket) {
        if ctx.player.can_use_game_master_blocks() {
            let PlayCtx { server, level, .. } = ctx;
            with_worldgen_clock(server, level, |level| {
                generate(level, packet.pos, packet.levels, packet.keep_jigsaws);
            });
        }
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static JIGSAW_GENERATE_HANDLER: &dyn PlayHandlers = &JigsawGenerateHandler;
