//! `net.minecraft.world.level.block.SpeleothemBlock` (abstract; subclasses `PointedDripstoneBlock`, `SulfurSpikeBlock`).
//!
//! The Java abstract hooks (`getStalactiteLandingSound`, `canGrow`, `blocksStalagmiteScan`, `getMaxGrowthLength`,
//! `randomTick` override of the dripstone) are the [`SpeleothemClass`] trait; [`Speleothem`] is the shared behaviour.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::collision::CollisionContext;
use crate::world::level::shapes::column;
use mc_core::phys::shapes::VoxelShape;
use mc_data::enums::SpeleothemThickness;
use std::sync::LazyLock;

const GROWTH_PROBABILITY_PER_RANDOM_TICK: f32 = 0.011377778;

static SHAPE_TIP_MERGE: LazyLock<VoxelShape> = LazyLock::new(|| column(6.0, 0.0, 16.0));
static SHAPE_TIP_UP: LazyLock<VoxelShape> = LazyLock::new(|| column(6.0, 0.0, 11.0));
/// `SHAPE_TIP_DOWN` (also read by `PointedDripstoneBlock` for the drip particle height).
pub static SHAPE_TIP_DOWN: LazyLock<VoxelShape> = LazyLock::new(|| column(6.0, 5.0, 16.0));
static SHAPE_FRUSTUM: LazyLock<VoxelShape> = LazyLock::new(|| column(8.0, 0.0, 16.0));
static SHAPE_MIDDLE: LazyLock<VoxelShape> = LazyLock::new(|| column(10.0, 0.0, 16.0));
static SHAPE_BASE: LazyLock<VoxelShape> = LazyLock::new(|| column(12.0, 0.0, 16.0));

/// The subclass-specific part of a speleothem block.
pub trait SpeleothemClass: Send + Sync + 'static {
    /// `blockToGrowOn.getBlock()`.
    fn block_to_grow_on(&self) -> Block;
    /// `getStalactiteLandingSound` (level event id).
    fn stalactite_landing_sound(&self) -> i32;
    /// `getMaxGrowthLength`.
    fn max_growth_length(&self) -> i32 {
        7
    }
    /// `canGrow`.
    fn can_grow(&self, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.above()).is_block(self.block_to_grow_on())
    }
    /// `blocksStalagmiteScan`.
    fn blocks_stalagmite_scan(&self, _level: &dyn LevelReader, _pos: BlockPos, _state: BlockState) -> bool {
        false
    }
    /// The part of a subclass' `randomTick` that runs before `super.randomTick`.
    fn before_random_tick(&self, _state: BlockState, _level: &mut ServerLevel, _pos: BlockPos) {}
}

/// `SpeleothemBlock` + its concrete subclass behaviour `C`.
pub struct Speleothem<C: SpeleothemClass> {
    pub block: Block,
    pub class: C,
}

/// `SpeleothemBlock.isSpeleothemWithDirection`.
pub fn is_speleothem_with_direction(block_state: BlockState, tip_direction: Direction) -> bool {
    block_state.is_tag("speleothems") && block_state.get(props::VERTICAL_DIRECTION) == tip_direction
}

/// `isValidSpeleothemPlacement` (`this` = the speleothem block).
pub fn is_valid_speleothem_placement(this: Block, level: &dyn LevelReader, pos: BlockPos, tip_direction: Direction) -> bool {
    let behind_pos = pos.relative(tip_direction.get_opposite());
    let behind_state = level.get_block_state(behind_pos);
    behind_state.is_face_sturdy_at(level, behind_pos, tip_direction) || (is_speleothem_with_direction(behind_state, tip_direction) && behind_state.is_block(this))
}

/// `calculateSpeleothemThickness`.
pub fn calculate_speleothem_thickness(this: Block, level: &dyn LevelReader, pos: BlockPos, tip_direction: Direction, merge_opposing_tips: bool) -> SpeleothemThickness {
    let base_direction = tip_direction.get_opposite();
    let in_front_state = level.get_block_state(pos.relative(tip_direction));
    if is_speleothem_with_direction(in_front_state, base_direction) && in_front_state.is_block(this) {
        if !merge_opposing_tips && in_front_state.get(props::SPELEOTHEM_THICKNESS) != SpeleothemThickness::TipMerge {
            SpeleothemThickness::Tip
        } else {
            SpeleothemThickness::TipMerge
        }
    } else if !is_speleothem_with_direction(in_front_state, tip_direction) {
        SpeleothemThickness::Tip
    } else {
        let in_front_thickness = in_front_state.get(props::SPELEOTHEM_THICKNESS);
        if in_front_thickness != SpeleothemThickness::Tip && in_front_thickness != SpeleothemThickness::TipMerge {
            let behind_state = level.get_block_state(pos.relative(base_direction));
            if !is_speleothem_with_direction(behind_state, tip_direction) { SpeleothemThickness::Base } else { SpeleothemThickness::Middle }
        } else {
            SpeleothemThickness::Frustum
        }
    }
}

/// `isStalagmite`.
pub fn is_stalagmite(state: BlockState) -> bool {
    is_speleothem_with_direction(state, Direction::Up)
}

/// `isStalactite`.
pub fn is_stalactite(state: BlockState) -> bool {
    is_speleothem_with_direction(state, Direction::Down)
}

/// `isTip(state, includeMergedTip)`.
pub fn is_tip(state: BlockState, include_merged_tip: bool) -> bool {
    if !state.is_tag("speleothems") {
        return false;
    }
    let thickness = state.get(props::SPELEOTHEM_THICKNESS);
    thickness == SpeleothemThickness::Tip || (include_merged_tip && thickness == SpeleothemThickness::TipMerge)
}

/// `isStalactiteStartPos`.
pub fn is_stalactite_start_pos(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    is_stalactite(state) && !level.get_block_state(pos.above()).is_block(state.block())
}

/// `isFreeHangingStalactite`.
pub fn is_free_hanging_stalactite(state: BlockState) -> bool {
    is_stalactite(state) && state.get(props::SPELEOTHEM_THICKNESS) == SpeleothemThickness::Tip && !state.get(props::WATERLOGGED)
}

/// `findBlockVertical`.
pub fn find_block_vertical(
    level: &dyn LevelReader,
    pos: BlockPos,
    direction: Direction,
    path_predicate: &dyn Fn(BlockPos, BlockState) -> bool,
    target_predicate: &dyn Fn(BlockState) -> bool,
    max_steps: i32,
) -> Option<BlockPos> {
    let mut mutable_pos = pos;
    for _ in 1..max_steps {
        mutable_pos = mutable_pos.relative(direction);
        let state = level.get_block_state(mutable_pos);
        if target_predicate(state) {
            return Some(mutable_pos);
        }
        if level.is_outside_build_height(mutable_pos.y()) || !path_predicate(mutable_pos, state) {
            return None;
        }
    }
    None
}

/// `findTip`.
pub fn find_tip(speleothem_state: BlockState, level: &dyn LevelReader, speleothem_pos: BlockPos, max_search_length: i32, include_merged_tip: bool) -> Option<BlockPos> {
    if is_tip(speleothem_state, include_merged_tip) {
        return Some(speleothem_pos);
    }
    let search_direction = speleothem_state.get(props::VERTICAL_DIRECTION);
    find_block_vertical(
        level,
        speleothem_pos,
        search_direction,
        &|_, state| state.is_block(speleothem_state.block()) && state.get(props::VERTICAL_DIRECTION) == search_direction,
        &|speleothem| is_tip(speleothem, include_merged_tip),
        max_search_length,
    )
}

impl<C: SpeleothemClass> Speleothem<C> {
    fn is_valid_placement(&self, level: &dyn LevelReader, pos: BlockPos, tip_direction: Direction) -> bool {
        is_valid_speleothem_placement(self.block, level, pos, tip_direction)
    }

    fn calculate_tip_direction(&self, level: &dyn LevelReader, pos: BlockPos, default_tip_direction: Direction) -> Option<Direction> {
        if self.is_valid_placement(level, pos, default_tip_direction) {
            Some(default_tip_direction)
        } else if self.is_valid_placement(level, pos, default_tip_direction.get_opposite()) {
            Some(default_tip_direction.get_opposite())
        } else {
            None
        }
    }

    fn is_unmerged_tip_with_direction(&self, state: BlockState, tip_direction: Direction) -> bool {
        is_tip(state, false) && state.get(props::VERTICAL_DIRECTION) == tip_direction && state.is_block(self.block)
    }

    fn can_tip_grow(&self, tip_state: BlockState, level: &dyn LevelReader, tip_pos: BlockPos) -> bool {
        let grow_direction = tip_state.get(props::VERTICAL_DIRECTION);
        let state_at_grow_pos = level.get_block_state(tip_pos.relative(grow_direction));
        if !state_at_grow_pos.get_fluid_state().is_empty() {
            false
        } else {
            state_at_grow_pos.is_air() || self.is_unmerged_tip_with_direction(state_at_grow_pos, grow_direction.get_opposite())
        }
    }

    fn create_speleothem(&self, level: &mut dyn LevelAccessor, pos: BlockPos, direction: Direction, thickness: SpeleothemThickness) {
        let state = self
            .block
            .default_state()
            .set(props::VERTICAL_DIRECTION, direction)
            .set(props::SPELEOTHEM_THICKNESS, thickness)
            .set(props::WATERLOGGED, level.get_fluid_state(pos).get_type() == mc_data::fluid::Fluid::WATER);
        level.set_block_and_update(pos, state);
    }

    fn create_merged_tips(&self, tip_state: BlockState, level: &mut dyn LevelAccessor, tip_pos: BlockPos) {
        let (stalactite_pos, stalagmite_pos) = if tip_state.get(props::VERTICAL_DIRECTION) == Direction::Up { (tip_pos.above(), tip_pos) } else { (tip_pos, tip_pos.below()) };
        self.create_speleothem(level, stalactite_pos, Direction::Down, SpeleothemThickness::TipMerge);
        self.create_speleothem(level, stalagmite_pos, Direction::Up, SpeleothemThickness::TipMerge);
    }

    fn grow(&self, level: &mut ServerLevel, grow_from_pos: BlockPos, grow_to_direction: Direction) {
        let target_pos = grow_from_pos.relative(grow_to_direction);
        let existing_state_at_target_pos = level.get_block_state(target_pos);
        if self.is_unmerged_tip_with_direction(existing_state_at_target_pos, grow_to_direction.get_opposite()) {
            self.create_merged_tips(existing_state_at_target_pos, level, target_pos);
        } else if existing_state_at_target_pos.is_air() || existing_state_at_target_pos.is_block(Block::WATER) {
            self.create_speleothem(level, target_pos, grow_to_direction, SpeleothemThickness::Tip);
        }
    }

    fn grow_stalagmite_below(&self, level: &mut ServerLevel, pos_above_stalagmite: BlockPos) {
        let mut pos = pos_above_stalagmite;
        for _ in 0..10 {
            pos = pos.below();
            let state = level.get_block_state(pos);
            if !state.get_fluid_state().is_empty() {
                return;
            }
            if self.is_unmerged_tip_with_direction(state, Direction::Up) && self.can_tip_grow(state, level, pos) {
                self.grow(level, pos, Direction::Up);
                return;
            }
            if self.is_valid_placement(level, pos, Direction::Up) && !helpers::is_water_at(level, pos.below()) {
                self.grow(level, pos.below(), Direction::Up);
                return;
            }
            if self.class.blocks_stalagmite_scan(level, pos, state) {
                return;
            }
        }
    }

    /// `growStalactiteOrStalagmiteIfPossible`.
    pub fn grow_stalactite_or_stalagmite_if_possible(&self, stalactite_start_state: BlockState, level: &mut ServerLevel, stalactite_start_pos: BlockPos) {
        if !self.class.can_grow(level, stalactite_start_pos) {
            return;
        }
        let Some(stalactite_tip_pos) = find_tip(stalactite_start_state, level, stalactite_start_pos, self.class.max_growth_length(), false) else { return };
        let stalactite_tip_state = level.get_block_state(stalactite_tip_pos);
        if is_free_hanging_stalactite(stalactite_tip_state) && self.can_tip_grow(stalactite_tip_state, level, stalactite_tip_pos) {
            if level.random().next_boolean() {
                self.grow(level, stalactite_tip_pos, Direction::Down);
            } else {
                self.grow_stalagmite_below(level, stalactite_tip_pos);
            }
        }
    }
}

impl<C: SpeleothemClass> BlockBehaviour for Speleothem<C> {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        self.is_valid_placement(level, pos, state.get(props::VERTICAL_DIRECTION))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction != Direction::Up && direction != Direction::Down {
            return state;
        }
        let tip_direction = state.get(props::VERTICAL_DIRECTION);
        if tip_direction == Direction::Down && level.has_scheduled_block_tick(pos, state.block()) {
            state
        } else if direction == tip_direction.get_opposite() && !self.can_survive(state, &*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), if tip_direction == Direction::Down { 2 } else { 1 });
            state
        } else {
            let merge_opposing_tips = state.get(props::SPELEOTHEM_THICKNESS) == SpeleothemThickness::TipMerge;
            state.set(props::SPELEOTHEM_THICKNESS, calculate_speleothem_thickness(state.block(), &*level, pos, tip_direction, merge_opposing_tips))
        }
    });
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let default_tip_direction = context.get_nearest_looking_vertical_direction().get_opposite();
        let tip_direction = self.calculate_tip_direction(level, pos, default_tip_direction)?;
        let merge_opposing_tips = !context.is_secondary_use_active();
        let thickness = calculate_speleothem_thickness(block, level, pos, tip_direction, merge_opposing_tips);
        Some(
            block
                .default_state()
                .set(props::VERTICAL_DIRECTION, tip_direction)
                .set(props::SPELEOTHEM_THICKNESS, thickness)
                .set(props::WATERLOGGED, level.get_fluid_state(pos).get_type() == mc_data::fluid::Fluid::WATER),
        )
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if is_stalagmite(state) && !self.can_survive(state, level, pos) {
            level.destroy_block(pos, true, None, 512);
        } else {
            spawn_falling_stalactite(state, level, pos);
        }
    }
    fn get_shape(&self, state: BlockState, _level: &dyn BlockGetter, pos: BlockPos, _context: &CollisionContext) -> VoxelShape {
        speleothem_shape(state, pos)
    }
    fn get_collision_shape(&self, state: BlockState, _level: &dyn BlockGetter, pos: BlockPos, _context: &CollisionContext) -> VoxelShape {
        speleothem_shape(state, pos)
    }
    fn is_collision_shape_full_block(&self, _state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        false
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        self.class.before_random_tick(state, level, pos);
        if level.random().next_float() < GROWTH_PROBABILITY_PER_RANDOM_TICK && is_stalactite_start_pos(state, level, pos) {
            self.grow_stalactite_or_stalagmite_if_possible(state, level, pos);
        }
    }
}

/// `SpeleothemBlock.getShape`: the thickness shape moved by the block's offset.
fn speleothem_shape(state: BlockState, pos: BlockPos) -> VoxelShape {
    let shape = match state.get(props::SPELEOTHEM_THICKNESS) {
        SpeleothemThickness::TipMerge => &*SHAPE_TIP_MERGE,
        SpeleothemThickness::Tip => {
            if state.get(props::VERTICAL_DIRECTION) == Direction::Down { &*SHAPE_TIP_DOWN } else { &*SHAPE_TIP_UP }
        }
        SpeleothemThickness::Frustum => &*SHAPE_FRUSTUM,
        SpeleothemThickness::Middle => &*SHAPE_MIDDLE,
        SpeleothemThickness::Base => &*SHAPE_BASE,
    };
    shape.move_vec(state.get_offset(pos))
}

/// `SpeleothemBlock.spawnFallingStalactite` (needs `FallingBlockEntity`).
fn spawn_falling_stalactite(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let mut fall_pos = pos;
    let mut fall_state = state;
    while is_stalactite(fall_state) {
        let entity = crate::world::entity::falling_block_entity::FallingBlockEntity::fall(level, fall_pos, fall_state);
        if is_tip(fall_state, true) {
            let size = (1 + pos.y() - fall_pos.y()).max(6);
            let damage_per_fall_distance = 1.0f32 * size as f32;
            entity.set_hurts_entities(damage_per_fall_distance, 40);
            break;
        }
        fall_pos = fall_pos.below();
        fall_state = level.get_block_state(fall_pos);
    }
}
