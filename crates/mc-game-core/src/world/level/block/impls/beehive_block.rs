//! `net.minecraft.world.level.block.BeehiveBlock` (`bee_nest`, `beehive`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::{BeeReleaseStatus, BeehiveBlockEntity};
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::{BlockPlaceContext, ItemStack};
use crate::world::item::enchantment::enchantment_tags::PREVENTS_BEE_SPAWNS_WHEN_MINING;
use crate::world::level::explosion::{Explosion, default_on_explosion_hit};
use mc_core::ident::Identifier;
use mc_core::phys::AABB;
use mc_data::EntityType;

pub const MAX_HONEY_LEVELS: i32 = 5;

pub struct BeehiveBlock;

/// `BeehiveBlock.resetHoneyLevel(level, state, pos)`.
pub fn reset_honey_level(level: &mut ServerLevel, state: BlockState, pos: BlockPos) {
    level.set_block_and_update(pos, state.set(props::LEVEL_HONEY, 0));
}

/// `BeehiveBlock.angerNearbyBees(level, pos)`: bees without a target near the hive pick a random nearby player.
pub fn anger_nearby_bees(level: &mut ServerLevel, pos: BlockPos) {
    let area = AABB::from_block_pos(&pos).inflate(8.0, 6.0, 8.0);
    let bees = level.get_entities_of_class(&area, |e| e.get_type() == EntityType::BEE, |_| true);
    if bees.is_empty() {
        return;
    }
    let players = level.get_entities_of_class(&area, |e| e.is_player(), |_| true);
    if players.is_empty() {
        return;
    }
    for bee in bees {
        let has_target = level.get_entity_mut(bee).and_then(|e| e.as_bee_access()).is_some_and(|b| b.get_target().is_some());
        if !has_target {
            // `Util.getRandom(playersToBeAngryAt, level.getRandom())`
            let target = players[level.random().next_int_bounded(players.len() as i32) as usize];
            if let Some(access) = level.get_entity_mut(bee).and_then(|e| e.as_bee_access()) {
                access.set_target(Some(target));
            }
        }
    }
}

impl BlockBehaviour for BeehiveBlock {
    fn on_explosion_hit(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, explosion: &dyn Explosion, on_hit: &mut dyn FnMut(ItemStack, BlockPos)) {
        default_on_explosion_hit(state, level, pos, explosion, on_hit);
        anger_nearby_bees(level, pos);
    }
    fn player_destroy(&self, level: &mut ServerLevel, player: &mut ServerPlayer, pos: BlockPos, state: BlockState, block_entity: Option<&dyn crate::world::block_entity::BlockEntity>, destroyed_with: &ItemStack) {
        crate::server::level::block_break::player_destroy(level, player, pos, state, block_entity, destroyed_with);
        if let Some(hive) = block_entity.and_then(|be| be.as_any().downcast_ref::<BeehiveBlockEntity>()) {
            let prevents = Identifier::try_parse(PREVENTS_BEE_SPAWNS_WHEN_MINING)
                .is_some_and(|tag| crate::world::item::enchantment_helper::has_tag(&level.registries, destroyed_with, &tag));
            if !prevents {
                // the snapshot of the destroyed hive: release its bees through a detached copy (the original is gone)
                let mut copy = BeehiveBlockEntity::new(pos, state);
                copy.set_bees(&mc_protocol::components::Bees { bees: hive.get_bees() });
                copy.saved_flower_pos = hive.saved_flower_pos;
                copy.empty_all_living_from_hive(level, Some(player.base_id()), state, BeeReleaseStatus::Emergency);
                level.update_neighbour_for_output_signal(pos, state.block());
                anger_nearby_bees(level, pos);
            }
            // CriteriaTriggers.BEE_NEST_DESTROYED.trigger(player, state, destroyedWith, hive.getOccupantCount()): with the triggers
        }
    }
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::HORIZONTAL_FACING, context.get_horizontal_direction().get_opposite()))
    }
    fn get_analog_output_signal(&self, state: BlockState, _level: &ServerLevel, _pos: BlockPos, _direction: Direction) -> i32 {
        state.get(props::LEVEL_HONEY)
    }
    fn get_clone_item_stack(&self, state: BlockState, _level: &ServerLevel, _pos: BlockPos, include_data: bool) -> ItemStack {
        let mut stack = match state.block().item() {
            Some(item) => ItemStack::new(item),
            None => ItemStack::empty(),
        };
        if include_data {
            helpers::set_block_state_property(&mut stack, "honey_level", &state.get(props::LEVEL_HONEY).to_string());
        }
        stack
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if helpers::is_instance_of(level.get_block_state(neighbour_pos), "FireBlock")
            && let Some(server_level) = level.as_server_level_mut()
        {
            server_level.with_block_entity(pos, |level, be| {
                if let Some(hive) = be.as_any_mut().downcast_mut::<BeehiveBlockEntity>() {
                    hive.empty_all_living_from_hive(level, None, state, crate::world::block_entity::BeeReleaseStatus::Emergency);
                }
            });
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BeehiveBlock", |_| Box::new(BeehiveBlock));
