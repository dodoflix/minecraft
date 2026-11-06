//! `net.minecraft.world.level.block.InfestedBlock`: the host block <-> infested block tables
//! (`infestedStateByHost`, `hostStateByInfested`, `isCompatibleHostBlock`).
//!
//! The silverfish that comes out when an infested block is broken (`spawnAfterBreak`) is created through the entity
//! factory; until the `Silverfish` type is registered nothing is spawned.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::item::enchantment::enchantment_tags;
use crate::world::item::{ItemStack, enchantment_helper};
use mc_data::EntityType;
use mc_data::enums::EntitySpawnReason;

pub struct InfestedBlock;

/// `InfestedBlock.getHostBlock()`: `infested_stone` -> `stone`, ... (every infested block's host has the same name without
/// the `infested_` prefix).
pub fn get_host_block(infested: Block) -> Block {
    let name = infested.data().name.replacen("infested_", "", 1);
    Block::by_name(&name).expect("host block of an infested block")
}

/// The `InfestedBlock` whose host is `host` (`BLOCK_BY_HOST_BLOCK.get(host)`).
fn infested_block_of_host(host: Block) -> Option<Block> {
    let name = host
        .data()
        .name
        .replacen("minecraft:", "minecraft:infested_", 1);
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
    get_host_block(infested_state.block())
        .default_state()
        .with_properties_of(infested_state)
}

/// `InfestedBlock.spawnInfestation`.
fn spawn_infestation(level: &mut ServerLevel, pos: BlockPos) {
    if !factory::has_factory(EntityType::SILVERFISH) {
        return;
    }
    let request = EntitySpawnRequest {
        reason: EntitySpawnReason::Triggered,
        ignore_checks: false,
    };
    if let Some(mut silverfish) = factory::create(EntityType::SILVERFISH, level, request) {
        silverfish.snap_to(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
            0.0,
            0.0,
        );
        let id = silverfish.id();
        level.add_fresh_entity(silverfish);
        level.broadcast_entity_event(id, 20); // `Mob.spawnAnim`
    }
}

/// `InfestedBlock.spawnAfterBreak` (`Block.spawnAfterBreak` itself does nothing).
pub fn spawn_after_break(level: &mut ServerLevel, pos: BlockPos, tool: &ItemStack) {
    let tag = mc_core::ident::Identifier::parse(enchantment_tags::PREVENTS_INFESTED_SPAWNS)
        .expect("valid tag id");
    if level.block_drops() && !enchantment_helper::has_tag(&level.registries, tool, &tag) {
        spawn_infestation(level, pos);
    }
}

impl BlockBehaviour for InfestedBlock {
    fn spawn_after_break(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        tool: &ItemStack,
        _drop_experience: bool,
    ) {
        spawn_after_break(level, pos, tool);
    }
}

register_block_class!("InfestedBlock", |_| Box::new(InfestedBlock));
