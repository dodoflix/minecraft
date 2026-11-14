//! `net.minecraft.util.SpawnUtil`: spawn a mob near a position by searching a column for a spot its [`Strategy`]
//! accepts.
//!
//! The creation step (`EntityType.create(level, null, pos, reason, false, false)`) is [`create_at`].

use super::{Entity, EntityId};
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::LevelAccessor as _;
use crate::world::level::block::{BlockStateExt, helpers};
use crate::world::level::collision::CollisionGetter;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::EntityType;
use mc_data::block::{Block, BlockState};
use mc_data::enums::EntitySpawnReason;

/// `SpawnUtil.Strategy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// `LEGACY_IRON_GOLEM`.
    LegacyIronGolem,
    /// `ON_TOP_OF_COLLIDER`.
    OnTopOfCollider,
    /// `ON_TOP_OF_COLLIDER_NO_LEAVES`.
    OnTopOfColliderNoLeaves,
}

impl Strategy {
    /// `canSpawnOn(level, pos, blockState, abovePos, aboveState)`.
    pub fn can_spawn_on(
        self,
        level: &ServerLevel,
        pos: BlockPos,
        block_state: BlockState,
        above_pos: BlockPos,
        above_state: BlockState,
    ) -> bool {
        match self {
            Strategy::LegacyIronGolem => {
                let block = block_state.block();
                let classes = block.data().classes;
                if block == Block::COBWEB
                    || block == Block::CACTUS
                    || block == Block::GLASS_PANE
                    || classes.contains(&"StainedGlassPaneBlock")
                    || classes.contains(&"StainedGlassBlock")
                    || classes.contains(&"LeavesBlock")
                    || block == Block::CONDUIT
                    || block == Block::ICE
                    || block == Block::TNT
                    || block == Block::GLOWSTONE
                    || block == Block::BEACON
                    || block == Block::SEA_LANTERN
                    || block == Block::FROSTED_ICE
                    || block == Block::TINTED_GLASS
                    || block == Block::GLASS
                {
                    false
                } else {
                    (above_state.is_air() || above_state.liquid())
                        && (block_state.is_solid() || block == Block::POWDER_SNOW)
                }
            }
            Strategy::OnTopOfCollider => {
                above_state.get_collision_shape(level, above_pos).is_empty()
                    && helpers::is_face_full(
                        &block_state.get_collision_shape(level, pos),
                        Direction::Up,
                    )
            }
            Strategy::OnTopOfColliderNoLeaves => {
                above_state.get_collision_shape(level, above_pos).is_empty()
                    && !block_state.is_tag("leaves")
                    && helpers::is_face_full(
                        &block_state.get_collision_shape(level, pos),
                        Direction::Up,
                    )
            }
        }
    }
}

/// `SpawnUtil.trySpawnMob(entityType, spawnReason, level, start, spawnAttempts, spawnRangeXZ, spawnRangeY, strategy, checkCollisions)`.
pub fn try_spawn_mob(
    level: &mut ServerLevel,
    entity_type: EntityType,
    spawn_reason: EntitySpawnReason,
    start: BlockPos,
    spawn_attempts: i32,
    spawn_range_xz: i32,
    spawn_range_y: i32,
    strategy: Strategy,
    check_collisions: bool,
) -> Option<EntityId> {
    for _ in 0..spawn_attempts {
        let random = level.random();
        let dx = mth::random_between_inclusive(random, -spawn_range_xz, spawn_range_xz);
        let dz = mth::random_between_inclusive(random, -spawn_range_xz, spawn_range_xz);
        let mut search_pos = start.offset(dx, spawn_range_y, dz);
        if level.is_within_world_border(search_pos)
            && move_to_possible_spawn_position(level, spawn_range_y, &mut search_pos, strategy)
            && (!check_collisions
                || level.no_collision(
                    None,
                    None,
                    &spawn_aabb(
                        entity_type,
                        f64::from(search_pos.x()) + 0.5,
                        f64::from(search_pos.y()),
                        f64::from(search_pos.z()) + 0.5,
                    ),
                ))
            && let Some(mob) = create_at(level, entity_type, search_pos, spawn_reason)
        {
            let valid = mob.check_spawn_rules(&*level, spawn_reason)
                && mob.check_spawn_obstruction(&*level);
            if valid {
                let id = mob.id();
                level.add_fresh_entity_with_passengers(vec![mob]);
                level.with_entity(id, |level, mob| mob.play_ambient_sound(level));
                return Some(id);
            }
        }
    }
    None
}

/// `EntityType.getSpawnAABB(x, y, z)`.
pub fn spawn_aabb(entity_type: EntityType, x: f64, y: f64, z: f64) -> AABB {
    let data = entity_type.data();
    let half_width = data.spawn_dimensions_scale * data.width / 2.0;
    let height = data.spawn_dimensions_scale * data.height;
    AABB::new(
        x - f64::from(half_width),
        y,
        z - f64::from(half_width),
        x + f64::from(half_width),
        y + f64::from(height),
        z + f64::from(half_width),
    )
}

/// `EntityType.create(level, null, spawnPos, spawnReason, false, false)`.
pub fn create_at(
    level: &mut ServerLevel,
    entity_type: EntityType,
    spawn_pos: BlockPos,
    spawn_reason: EntitySpawnReason,
) -> Option<Box<dyn Entity>> {
    super::spawn_config::create_at(
        level,
        entity_type,
        None,
        spawn_pos,
        spawn_reason,
        false,
        false,
    )
}

/// `moveToPossibleSpawnPosition(level, spawnRangeY, searchPos, strategy)`.
fn move_to_possible_spawn_position(
    level: &ServerLevel,
    spawn_range_y: i32,
    search_pos: &mut BlockPos,
    strategy: Strategy,
) -> bool {
    let mut above_state = level.get_block_state(*search_pos);
    for _ in -spawn_range_y..=spawn_range_y {
        *search_pos = search_pos.below();
        let above_pos = search_pos.above();
        let current_state = level.get_block_state(*search_pos);
        if strategy.can_spawn_on(level, *search_pos, current_state, above_pos, above_state) {
            *search_pos = search_pos.above();
            return true;
        }
        above_state = current_state;
    }
    false
}
