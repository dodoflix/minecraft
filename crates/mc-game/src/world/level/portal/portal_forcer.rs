//! `net.minecraft.world.level.portal.PortalForcer`.
//!
//! Vanilla's `ServerLevel.getBlockState` loads the chunk synchronously; `ServerLevel::get_block_state(&self)` cannot, so every
//! block read of the forcer goes through [`PortalForcer::block_state`] which loads the full chunk first.
use crate::server::level::ServerLevel;
use crate::util::block_util::FoundRectangle;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::{Axis, AxisDirection, Direction};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::props;
use mc_world::heightmap::HeightmapType;

pub const TICKET_RADIUS: i32 = 3;
const NETHER_PORTAL_RADIUS: i32 = 16;
const OVERWORLD_PORTAL_RADIUS: i32 = 128;

pub struct PortalForcer<'a> {
    level: &'a mut ServerLevel,
}

impl ServerLevel {
    /// `getPortalForcer()`.
    pub fn get_portal_forcer(&mut self) -> PortalForcer<'_> {
        PortalForcer { level: self }
    }

    /// `isSpawningMonsters()`.
    pub fn is_spawning_monsters(&self) -> bool {
        self.spawn_mobs() && self.spawn_monsters()
    }

    /// `environmentAttributes().getValue(NETHER_PORTAL_SPAWNS_PIGLINS, pos)`: a dimension constant (no biome layer changes it).
    pub fn nether_portal_spawns_piglins(&self) -> bool {
        let env = crate::world::attribute::AttributeEnv { clock_total_ticks: &|_| 0, rain_level: 0.0, thunder_level: 0.0 };
        self.tick_state.attributes.get_bool("gameplay/nether_portal_spawns_piglin", &env)
    }

    /// `DimensionType.getTeleportationScale(this.dimensionType(), other.dimensionType())`.
    pub fn teleportation_scale_to(&self, other: &ServerLevel) -> f64 {
        let scale = |l: &ServerLevel| l.dimension_type.as_ref().map_or(1.0, |t| t.coordinate_scale);
        scale(self) / scale(other)
    }

    /// `getLogicalHeight()` (`dimensionType().logicalHeight()`).
    pub fn get_logical_height(&self) -> i32 {
        self.dimension_type.as_ref().map_or_else(|| self.get_height(), |t| t.logical_height)
    }
}

impl PortalForcer<'_> {
    /// `ServerLevel.getBlockState(pos)`: loads the chunk when it is not there.
    pub fn block_state(&mut self, pos: BlockPos) -> BlockState {
        if !self.level.is_outside_build_height(pos.y()) && self.level.chunk_now(pos.x() >> 4, pos.z() >> 4).is_none() {
            self.level.chunk_blocking_mut(pos.x() >> 4, pos.z() >> 4);
        }
        self.level.get_block_state(pos)
    }

    /// `findClosestPortalPosition(approximateExitPos, toNether, worldBorder)`: the border is the one of this level.
    pub fn find_closest_portal_position(&mut self, approximate_exit_pos: BlockPos, to_nether: bool) -> Option<BlockPos> {
        let radius = if to_nether { NETHER_PORTAL_RADIUS } else { OVERWORLD_PORTAL_RADIUS };
        let candidates = self.level.nether_portal_pois_in_square(approximate_exit_pos, radius);
        let mut best: Option<(f64, BlockPos)> = None;
        for pos in candidates {
            if !self.level.world_border().is_within_bounds_block(&pos) {
                continue;
            }
            if !self.block_state(pos).has_property(props::HORIZONTAL_AXIS) {
                continue;
            }
            // `Comparator.comparingDouble(distSqr).thenComparingInt(y)`; `Stream.min` keeps the first of equal elements
            let dist = pos.dist_sqr(&approximate_exit_pos);
            let better = match best {
                None => true,
                Some((best_dist, best_pos)) => dist < best_dist || (dist == best_dist && pos.y() < best_pos.y()),
            };
            if better {
                best = Some((dist, pos));
            }
        }
        best.map(|(_, pos)| pos)
    }

    /// `createPortal(origin, portalAxis)`.
    pub fn create_portal(&mut self, origin: BlockPos, portal_axis: Axis) -> Option<FoundRectangle> {
        let direction = Direction::get(AxisDirection::Positive, portal_axis);
        let mut closest_full_distance_sqr = -1.0f64;
        let mut closest_full_position: Option<BlockPos> = None;
        let mut closest_partial_distance_sqr = -1.0f64;
        let mut closest_partial_position: Option<BlockPos> = None;
        let max_placeable_y = self.level.get_max_y().min(self.level.get_min_y() + self.level.get_logical_height() - 1);
        let min_y = self.level.get_min_y();

        for column_pos in BlockPos::spiral_around(&origin, 16, Direction::East, Direction::South) {
            let height = max_placeable_y.min(self.level.get_height_at(HeightmapType::MotionBlocking, column_pos.x(), column_pos.z()));
            if self.level.world_border().is_within_bounds_block(&column_pos) && self.level.world_border().is_within_bounds_block(&column_pos.relative(direction)) {
                let mut y = height;
                while y >= min_y {
                    let mut at = column_pos.at_y(y);
                    if self.can_portal_replace_block(at) {
                        let first_empty_y = y;

                        while y > min_y && {
                            at = at.below();
                            self.can_portal_replace_block(at)
                        } {
                            y -= 1;
                        }

                        if y + 4 <= max_placeable_y {
                            let delta_y = first_empty_y - y;
                            if delta_y <= 0 || delta_y >= 3 {
                                let host = column_pos.at_y(y);
                                if self.can_host_frame(host, direction, 0) {
                                    let distance = origin.dist_sqr(&host);
                                    if self.can_host_frame(host, direction, -1)
                                        && self.can_host_frame(host, direction, 1)
                                        && (closest_full_distance_sqr == -1.0 || closest_full_distance_sqr > distance)
                                    {
                                        closest_full_distance_sqr = distance;
                                        closest_full_position = Some(host);
                                    }

                                    if closest_full_distance_sqr == -1.0 && (closest_partial_distance_sqr == -1.0 || closest_partial_distance_sqr > distance) {
                                        closest_partial_distance_sqr = distance;
                                        closest_partial_position = Some(host);
                                    }
                                }
                            }
                        }
                    }
                    y -= 1;
                }
            }
        }

        if closest_full_distance_sqr == -1.0 && closest_partial_distance_sqr != -1.0 {
            closest_full_position = closest_partial_position;
            closest_full_distance_sqr = closest_partial_distance_sqr;
        }

        let closest_full_position = if closest_full_distance_sqr == -1.0 {
            let min_start_y = (self.level.get_min_y() + 1).max(70);
            let max_start_y = max_placeable_y - 9;
            if max_start_y < min_start_y {
                return None;
            }

            let start = BlockPos::new(origin.x() - direction.get_step_x(), mth::clamp_i32(origin.y(), min_start_y, max_start_y), origin.z() - direction.get_step_z());
            let closest = self.level.world_border().clamp_to_bounds_block(&start);
            let clock_wise = direction.get_clockwise();

            for box_ in -1..2 {
                for width in 0..2 {
                    for height in -1..3 {
                        let block_state = if height < 0 { Block::OBSIDIAN.default_state() } else { Block::AIR.default_state() };
                        let pos = closest.offset(
                            width * direction.get_step_x() + box_ * clock_wise.get_step_x(),
                            height,
                            width * direction.get_step_z() + box_ * clock_wise.get_step_z(),
                        );
                        self.level.set_block_and_update(pos, block_state);
                    }
                }
            }
            closest
        } else {
            closest_full_position.expect("a distance was recorded with its position")
        };

        for width in -1..3 {
            for height in -1..4 {
                if width == -1 || width == 2 || height == -1 || height == 3 {
                    let pos = closest_full_position.offset(width * direction.get_step_x(), height, width * direction.get_step_z());
                    self.level.set_block_and_update(pos, Block::OBSIDIAN.default_state());
                }
            }
        }

        let portal_block_state = Block::NETHER_PORTAL.default_state().set(props::HORIZONTAL_AXIS, portal_axis);

        for width in 0..2 {
            for height in 0..3 {
                let pos = closest_full_position.offset(width * direction.get_step_x(), height, width * direction.get_step_z());
                self.level.set_block_flags(pos, portal_block_state, 18);
            }
        }

        Some(FoundRectangle { min_corner: closest_full_position, axis1_size: 2, axis2_size: 3 })
    }

    fn can_portal_replace_block(&mut self, pos: BlockPos) -> bool {
        let block_state = self.block_state(pos);
        block_state.can_be_replaced() && block_state.get_fluid_state().is_empty()
    }

    fn can_host_frame(&mut self, origin: BlockPos, direction: Direction, offset: i32) -> bool {
        let clock_wise = direction.get_clockwise();

        for width in -1..3 {
            for height in -1..4 {
                let pos = origin.offset(direction.get_step_x() * width + clock_wise.get_step_x() * offset, height, direction.get_step_z() * width + clock_wise.get_step_z() * offset);
                if height < 0 && !self.block_state(pos).is_solid() {
                    return false;
                }

                if height >= 0 && !self.can_portal_replace_block(pos) {
                    return false;
                }
            }
        }

        true
    }
}
