//! `net.minecraft.world.level.block.WitherSkullBlock` (extends `SkullBlock`) and `WitherWallSkullBlock` (extends
//! `WallSkullBlock`): placing the last head of the wither pattern summons the wither.
use super::{skull_block::SkullBlock, wall_skull_block::WallSkullBlock};
use crate::world::block_entity::SkullBlockEntity;
use mc_data::block::PathComputationType;
use mc_data::{EntityType, Item};
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::BlockEntity;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::entity::boss::wither::wither_boss::WitherBoss;
use mc_game_core::world::item::{BlockPlaceContext, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::pattern::{BlockPattern, BlockPatternBuilder};
use mc_game_core::world::level::block::pattern_blocks;
use mc_game_core::world::level::redstone::Orientation;
use mc_world::level_storage::Difficulty;
use std::sync::OnceLock;

pub struct WitherSkullBlock;
pub struct WitherWallSkullBlock;

fn wither_full() -> &'static BlockPattern {
    static PATTERN: OnceLock<BlockPattern> = OnceLock::new();
    PATTERN.get_or_init(|| {
        BlockPatternBuilder::start()
            .aisle(&["^^^", "###", "~#~"])
            .where_('#', |block, level| {
                block
                    .get_state(level)
                    .is_some_and(|s| s.is_tag("wither_summon_base_blocks"))
            })
            .where_('^', |block, level| {
                block.get_state(level).is_some_and(|s| {
                    s.block() == Block::WITHER_SKELETON_SKULL
                        || s.block() == Block::WITHER_SKELETON_WALL_SKULL
                })
            })
            .where_('~', |block, level| {
                block.get_state(level).is_some_and(|s| s.is_air())
            })
            .build()
    })
}

fn wither_base() -> &'static BlockPattern {
    static PATTERN: OnceLock<BlockPattern> = OnceLock::new();
    PATTERN.get_or_init(|| {
        BlockPatternBuilder::start()
            .aisle(&["   ", "###", "~#~"])
            .where_('#', |block, level| {
                block
                    .get_state(level)
                    .is_some_and(|s| s.is_tag("wither_summon_base_blocks"))
            })
            .where_('~', |block, level| {
                block.get_state(level).is_some_and(|s| s.is_air())
            })
            .build()
    })
}

/// `WitherSkullBlock.checkSpawn(level, pos)`.
pub fn check_spawn(level: &mut ServerLevel, pos: BlockPos) {
    let Some(state) = level
        .get_block_entity(pos)
        .and_then(|be| be.as_any().downcast_ref::<SkullBlockEntity>())
        .map(|skull| skull.get_block_state())
    else {
        return;
    };
    let correct_block = state.block() == Block::WITHER_SKELETON_SKULL
        || state.block() == Block::WITHER_SKELETON_WALL_SKULL;
    if !(correct_block
        && pos.y() >= level.get_min_y()
        && level.level_data.get_difficulty() != Difficulty::Peaceful)
    {
        return;
    }
    let Some(mut found) = wither_full().find(&*level, pos) else {
        return;
    };
    let Some(mut wither) = pattern_blocks::create_triggered(level, EntityType::WITHER) else {
        return;
    };
    pattern_blocks::clear_pattern_blocks(level, &mut found);
    let spawn_pos = found.get_block(1, 2, 0).get_pos();
    let rot = if found.get_forwards().get_axis() == Axis::X {
        0.0
    } else {
        90.0
    };
    wither.snap_to(
        f64::from(spawn_pos.x()) + 0.5,
        f64::from(spawn_pos.y()) + 0.55,
        f64::from(spawn_pos.z()) + 0.5,
        rot,
        0.0,
    );
    if let Some(boss) = wither.as_any_mut().downcast_mut::<WitherBoss>() {
        boss.mob.living.y_body_rot = rot;
        boss.make_invulnerable();
    }
    pattern_blocks::trigger_summoned_entity(level, &*wither, 50.0);
    pattern_blocks::add_fresh_entity(level, wither);
    pattern_blocks::update_pattern_blocks(level, &mut found);
}

/// `WitherSkullBlock.canSpawnMob(level, pos, itemStack)` (dispensing a wither skull).
pub fn can_spawn_mob(level: &ServerLevel, pos: BlockPos, stack: &ItemStack) -> bool {
    stack.is(Item::WITHER_SKELETON_SKULL)
        && pos.y() >= level.get_min_y() + 2
        && level.level_data.get_difficulty() != Difficulty::Peaceful
        && wither_base().find(level, pos).is_some()
}

impl BlockBehaviour for WitherSkullBlock {
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        SkullBlock.is_pathfindable(state, ty)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        SkullBlock.get_state_for_placement(block, level, context)
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        moved_by_piston: bool,
    ) {
        SkullBlock.neighbor_changed(state, level, pos, block, orientation, moved_by_piston)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        SkullBlock.rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        SkullBlock.mirror(state, mirror)
    }
    fn set_placed_by(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        placer: Option<&mut ServerPlayer>,
        _stack: &ItemStack,
    ) {
        // the summoned wither's trigger awards the placing player, so it is lent
        match placer {
            Some(player) => level.lend(player, |level| check_spawn(level, pos)),
            None => check_spawn(level, pos),
        }
    }
}

impl BlockBehaviour for WitherWallSkullBlock {
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        WallSkullBlock.is_pathfindable(state, ty)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        WallSkullBlock.get_state_for_placement(block, level, context)
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        moved_by_piston: bool,
    ) {
        WallSkullBlock.neighbor_changed(state, level, pos, block, orientation, moved_by_piston)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        WallSkullBlock.rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        WallSkullBlock.mirror(state, mirror)
    }
    fn set_placed_by(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        placer: Option<&mut ServerPlayer>,
        _stack: &ItemStack,
    ) {
        // the summoned wither's trigger awards the placing player, so it is lent
        match placer {
            Some(player) => level.lend(player, |level| check_spawn(level, pos)),
            None => check_spawn(level, pos),
        }
    }
}

register_block_class!("WitherSkullBlock", |_| Box::new(WitherSkullBlock));
register_block_class!("WitherWallSkullBlock", |_| Box::new(WitherWallSkullBlock));
