//! `net.minecraft.world.level.block.InfestedBlock`: the host block <-> infested block tables
//! (`infestedStateByHost`, `hostStateByInfested`, `isCompatibleHostBlock`).
//!
//! The silverfish that comes out when an infested block is broken (`spawnAfterBreak`) is wired with the entity
//! hooks, see STATUS.md.
use super::prelude::*;
use crate::world::item::BlockPlaceContext;

pub struct InfestedBlock;

/// `InfestedBlock.getHostBlock()`: `infested_stone` -> `stone`, ... (every infested block's host has the same name without
/// the `infested_` prefix).
pub fn get_host_block(infested: Block) -> Block {
    let name = infested.data().name.replacen("infested_", "", 1);
    Block::by_name(&name).expect("host block of an infested block")
}

/// The `InfestedBlock` whose host is `host` (`BLOCK_BY_HOST_BLOCK.get(host)`).
fn infested_block_of_host(host: Block) -> Option<Block> {
    let name = host.data().name.replacen("minecraft:", "minecraft:infested_", 1);
    Block::by_name(&name).filter(|b| helpers::is_instance_of(b.default_state(), "InfestedBlock"))
}

/// `InfestedBlock.isCompatibleHostBlock(state)`.
pub fn is_compatible_host_block(state: BlockState) -> bool {
    infested_block_of_host(state.block()).is_some()
}

/// `InfestedBlock.infestedStateByHost(hostState)`.
pub fn infested_state_by_host(host_state: BlockState) -> BlockState {
    let infested = infested_block_of_host(host_state.block()).expect("compatible host block");
    infested.default_state().with_properties_of(host_state)
}

/// `InfestedBlock.hostStateByInfested(infestedState)`.
pub fn host_state_by_infested(infested_state: BlockState) -> BlockState {
    get_host_block(infested_state.block()).default_state().with_properties_of(infested_state)
}

impl BlockBehaviour for InfestedBlock {}

register_block_class!("InfestedBlock", |_| Box::new(InfestedBlock));
