//! `BlockBehaviour.BlockStateBase` methods that dispatch to the block's [`BlockBehaviour`] (everything that is plain
//! static data is an inherent method of `mc_data::block::BlockState`), as an extension trait so call sites read like
//! vanilla (`state.update_shape(..)`, `state.tick(..)`).

use super::behaviour;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::collision::CollisionContext;
use crate::world::level::redstone::Orientation;
use crate::world::level::shapes::{is_face_full, is_shape_full_block, voxel_shape};
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::mirror::Mirror;
use mc_core::phys::shapes::VoxelShape;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::rotation::Rotation;
use mc_data::block::{Block, BlockState, OffsetType, PathComputationType, SupportType, block_flags};
use mc_data::fluid::Fluid;

/// `BlockBehaviour.UPDATE_SHAPE_ORDER`.
pub const UPDATE_SHAPE_ORDER: [Direction; 6] =
    [Direction::West, Direction::East, Direction::North, Direction::South, Direction::Down, Direction::Up];

pub trait BlockStateExt: Copy {
    /// `state.is(block)`.
    fn is_block(self, block: Block) -> bool;
    /// `state.is(tag)` for a block tag name (`minecraft:logs` or `logs`); hash lookup per call.
    fn is_tag(self, tag: &str) -> bool;
    /// `hasBlockEntity()` (`getBlock() instanceof EntityBlock`).
    fn has_block_entity(self) -> bool;

    fn update_shape(self, level: &mut dyn LevelAccessor, pos: BlockPos, direction: Direction, neighbour_pos: BlockPos, neighbour_state: BlockState) -> BlockState;
    fn handle_neighbor_changed(self, level: &mut ServerLevel, pos: BlockPos, block: Block, orientation: Option<Orientation>, moved_by_piston: bool);
    /// `updateNeighbourShapes(level, pos, flags, limit)`.
    fn update_neighbour_shapes(self, level: &mut dyn LevelAccessor, pos: BlockPos, update_flags: u32, update_limit: i32);
    fn update_indirect_neighbour_shapes(self, level: &mut dyn LevelAccessor, pos: BlockPos, update_flags: u32, update_limit: i32);
    fn on_place(self, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, moved_by_piston: bool);
    fn affect_neighbors_after_removal(self, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool);
    fn tick(self, level: &mut ServerLevel, pos: BlockPos);
    fn random_tick(self, level: &mut ServerLevel, pos: BlockPos);
    fn trigger_event(self, level: &mut ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool;
    fn entity_inside(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier, is_precise: bool);
    /// `getBlock().stepOn(level, pos, this, entity)`.
    fn step_on(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity);
    /// `getBlock().fallOn(level, this, pos, entity, fallDistance)`.
    fn fall_on(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64);
    /// `getBlock().bounceOn(level, this, pos, entity, fallDistance)`.
    fn bounce_on(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64);
    fn is_pathfindable_by(self, ty: PathComputationType) -> bool;
    fn can_survive(self, level: &dyn LevelReader, pos: BlockPos) -> bool;
    fn can_be_replaced_by_fluid(self, fluid: Fluid) -> bool;
    /// `canBeReplaced(context)`.
    fn can_be_replaced_by(self, context: &crate::world::item::BlockPlaceContext) -> bool;
    fn should_changed_state_keep_block_entity(self, old_state: BlockState) -> bool;
    fn rotate(self, rotation: Rotation) -> BlockState;
    fn mirror(self, mirror: Mirror) -> BlockState;
    fn get_seed(self, pos: BlockPos) -> i64;

    fn get_signal(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32;
    fn get_direct_signal(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32;
    fn get_own_signal(self, level: &dyn BlockGetter, pos: BlockPos) -> i32;
    fn should_redstone_wire_connect_to(self, level: &dyn BlockGetter, pos: BlockPos, direction: Option<Direction>) -> bool;
    fn get_analog_output_signal(self, level: &ServerLevel, pos: BlockPos, direction: Direction) -> i32;

    // shapes
    fn get_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape;
    fn get_shape_ctx(self, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape;
    fn get_collision_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape;
    fn get_collision_shape_ctx(self, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape;
    fn get_visual_shape(self, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape;
    fn get_block_support_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape;
    fn get_interaction_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape;
    fn get_entity_inside_collision_shape(self, level: &dyn BlockGetter, pos: BlockPos, entity: &dyn Entity) -> VoxelShape;
    /// `getOcclusionShape()`.
    fn get_occlusion_shape(self) -> VoxelShape;
    /// `getFaceOcclusionShape(direction)`.
    fn get_face_occlusion_shape(self, direction: Direction) -> VoxelShape;
    fn is_collision_shape_full_block_at(self, level: &dyn BlockGetter, pos: BlockPos) -> bool;
    fn is_face_sturdy_at(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> bool;
    fn is_face_sturdy_support(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction, support: SupportType) -> bool;
    fn is_suffocating_at(self, level: &dyn BlockGetter, pos: BlockPos) -> bool;
    /// `getOffset(pos)` (the block's `OffsetType`).
    fn get_offset(self, pos: BlockPos) -> Vec3;
    /// `withPropertiesOf(source)`: copy every property both blocks have.
    fn with_properties_of(self, source: BlockState) -> BlockState;
    /// `getPostProcessPos(level, pos)` (`BlockBehaviour.Properties.postProcess`): the position whose generation
    /// post-processing `WorldGenRegion.setBlock` marks (only brown/red mushroom: the block itself; soul sand/magma block:
    /// the block above), `None` for every other block.
    fn get_post_process_pos(self, level: &dyn BlockGetter, pos: BlockPos) -> Option<BlockPos>;
}

impl BlockStateExt for BlockState {
    fn is_block(self, block: Block) -> bool {
        self.block() == block
    }
    fn is_tag(self, tag: &str) -> bool {
        self.block().is_in_tag(tag)
    }
    fn has_block_entity(self) -> bool {
        self.block().data().has(block_flags::HAS_BLOCK_ENTITY)
    }

    fn update_shape(self, level: &mut dyn LevelAccessor, pos: BlockPos, direction: Direction, neighbour_pos: BlockPos, neighbour_state: BlockState) -> BlockState {
        behaviour(self.block()).update_shape(self, level, pos, direction, neighbour_pos, neighbour_state)
    }
    fn handle_neighbor_changed(self, level: &mut ServerLevel, pos: BlockPos, block: Block, orientation: Option<Orientation>, moved_by_piston: bool) {
        behaviour(self.block()).neighbor_changed(self, level, pos, block, orientation, moved_by_piston);
    }
    fn update_neighbour_shapes(self, level: &mut dyn LevelAccessor, pos: BlockPos, update_flags: u32, update_limit: i32) {
        for direction in UPDATE_SHAPE_ORDER {
            level.neighbor_shape_changed(direction.get_opposite(), pos.relative(direction), pos, self, update_flags, update_limit);
        }
    }
    fn update_indirect_neighbour_shapes(self, level: &mut dyn LevelAccessor, pos: BlockPos, update_flags: u32, update_limit: i32) {
        behaviour(self.block()).update_indirect_neighbour_shapes(self, level, pos, update_flags, update_limit);
    }
    fn on_place(self, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, moved_by_piston: bool) {
        behaviour(self.block()).on_place(self, level, pos, old_state, moved_by_piston);
    }
    fn affect_neighbors_after_removal(self, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) {
        behaviour(self.block()).affect_neighbors_after_removal(self, level, pos, moved_by_piston);
    }
    fn tick(self, level: &mut ServerLevel, pos: BlockPos) {
        behaviour(self.block()).tick(self, level, pos);
    }
    fn random_tick(self, level: &mut ServerLevel, pos: BlockPos) {
        behaviour(self.block()).random_tick(self, level, pos);
    }
    fn trigger_event(self, level: &mut ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool {
        behaviour(self.block()).trigger_event(self, level, pos, b0, b1)
    }
    fn entity_inside(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier, is_precise: bool) {
        behaviour(self.block()).entity_inside(self, level, pos, entity, effect_applier, is_precise);
    }
    fn step_on(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity) {
        behaviour(self.block()).step_on(level, pos, self, entity);
    }
    fn fall_on(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64) {
        behaviour(self.block()).fall_on(level, self, pos, entity, fall_distance);
    }
    fn bounce_on(self, level: &mut ServerLevel, pos: BlockPos, entity: &mut dyn Entity, fall_distance: f64) {
        behaviour(self.block()).bounce_on(level, self, pos, entity, fall_distance);
    }
    fn is_pathfindable_by(self, ty: PathComputationType) -> bool {
        behaviour(self.block()).is_pathfindable(self, ty)
    }
    fn can_survive(self, level: &dyn LevelReader, pos: BlockPos) -> bool {
        behaviour(self.block()).can_survive(self, level, pos)
    }
    fn can_be_replaced_by_fluid(self, fluid: Fluid) -> bool {
        behaviour(self.block()).can_be_replaced_by_fluid(self, fluid)
    }
    fn can_be_replaced_by(self, context: &crate::world::item::BlockPlaceContext) -> bool {
        behaviour(self.block()).can_be_replaced(self, context)
    }
    fn should_changed_state_keep_block_entity(self, old_state: BlockState) -> bool {
        behaviour(self.block()).should_changed_state_keep_block_entity(old_state)
    }
    fn rotate(self, rotation: Rotation) -> BlockState {
        behaviour(self.block()).rotate(self, rotation)
    }
    fn mirror(self, mirror: Mirror) -> BlockState {
        behaviour(self.block()).mirror(self, mirror)
    }
    fn get_seed(self, pos: BlockPos) -> i64 {
        behaviour(self.block()).get_seed(self, pos)
    }

    fn get_signal(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        behaviour(self.block()).get_signal(self, level, pos, direction)
    }
    fn get_direct_signal(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        behaviour(self.block()).get_direct_signal(self, level, pos, direction)
    }
    fn get_own_signal(self, level: &dyn BlockGetter, pos: BlockPos) -> i32 {
        behaviour(self.block()).own_signal(self, level, pos)
    }
    fn should_redstone_wire_connect_to(self, level: &dyn BlockGetter, pos: BlockPos, direction: Option<Direction>) -> bool {
        behaviour(self.block()).should_redstone_wire_connect_to(self, level, pos, direction)
    }
    fn get_analog_output_signal(self, level: &ServerLevel, pos: BlockPos, direction: Direction) -> i32 {
        behaviour(self.block()).get_analog_output_signal(self, level, pos, direction)
    }

    fn get_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        self.get_shape_ctx(level, pos, &CollisionContext::empty())
    }
    fn get_shape_ctx(self, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        behaviour(self.block()).get_shape(self, level, pos, context)
    }
    fn get_collision_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        // Vanilla returns the cached collision shape for non-dynamic blocks (same value as the dispatch below).
        if self.block().data().has(block_flags::DYNAMIC_SHAPE) {
            self.get_collision_shape_ctx(level, pos, &CollisionContext::empty())
        } else {
            voxel_shape(self.collision_shape())
        }
    }
    fn get_collision_shape_ctx(self, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        behaviour(self.block()).get_collision_shape(self, level, pos, context)
    }
    fn get_visual_shape(self, level: &dyn BlockGetter, pos: BlockPos, context: &CollisionContext) -> VoxelShape {
        behaviour(self.block()).get_visual_shape(self, level, pos, context)
    }
    fn get_block_support_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        behaviour(self.block()).get_block_support_shape(self, level, pos)
    }
    fn get_interaction_shape(self, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
        behaviour(self.block()).get_interaction_shape(self, level, pos)
    }
    fn get_entity_inside_collision_shape(self, level: &dyn BlockGetter, pos: BlockPos, entity: &dyn Entity) -> VoxelShape {
        behaviour(self.block()).get_entity_inside_collision_shape(self, level, pos, entity)
    }
    fn get_occlusion_shape(self) -> VoxelShape {
        voxel_shape(self.occlusion_shape())
    }
    fn get_face_occlusion_shape(self, direction: Direction) -> VoxelShape {
        // `occlusionShapesByFace`: empty / full block / the face shape of the occlusion shape.
        let shape = self.get_occlusion_shape();
        if shape.is_empty() {
            shape
        } else if is_shape_full_block(&shape) {
            mc_core::phys::shapes::Shapes::block()
        } else {
            shape.get_face_shape(direction)
        }
    }
    fn is_collision_shape_full_block_at(self, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        if self.block().data().has(block_flags::DYNAMIC_SHAPE) {
            behaviour(self.block()).is_collision_shape_full_block(self, level, pos)
        } else {
            self.is_collision_shape_full_block()
        }
    }
    fn is_face_sturdy_at(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> bool {
        self.is_face_sturdy_support(level, pos, direction, SupportType::Full)
    }
    fn is_face_sturdy_support(self, level: &dyn BlockGetter, pos: BlockPos, direction: Direction, support: SupportType) -> bool {
        if self.block().data().has(block_flags::DYNAMIC_SHAPE) {
            is_supporting(self, level, pos, direction, support)
        } else {
            self.is_face_sturdy(direction, support)
        }
    }
    fn is_suffocating_at(self, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        // `isSuffocating` predicates in 26.3 only look at the state and block tags (extracted per state).
        self.is_suffocating()
    }
    fn get_offset(self, pos: BlockPos) -> Vec3 {
        let data = self.block().data();
        match data.offset_type {
            OffsetType::None => Vec3::ZERO,
            OffsetType::Xz => {
                let seed = mth::get_seed_xyz(pos.x(), 0, pos.z());
                let max_h = f64::from(data.max_horizontal_offset);
                let x = mth::clamp_f64((f64::from((seed & 15) as f32 / 15.0) - 0.5) * 0.5, -max_h, max_h);
                let z = mth::clamp_f64((f64::from((seed >> 8 & 15) as f32 / 15.0) - 0.5) * 0.5, -max_h, max_h);
                Vec3::new(x, 0.0, z)
            }
            OffsetType::Xyz => {
                let seed = mth::get_seed_xyz(pos.x(), 0, pos.z());
                let y = f64::from((seed >> 4 & 15) as f32 / 15.0 - 1.0) * f64::from(data.max_vertical_offset);
                let max_h = f64::from(data.max_horizontal_offset);
                let x = mth::clamp_f64((f64::from((seed & 15) as f32 / 15.0) - 0.5) * 0.5, -max_h, max_h);
                let z = mth::clamp_f64((f64::from((seed >> 8 & 15) as f32 / 15.0) - 0.5) * 0.5, -max_h, max_h);
                Vec3::new(x, y, z)
            }
        }
    }
    fn with_properties_of(self, source: BlockState) -> BlockState {
        let mut result = self;
        for def in source.block().properties() {
            if let Some(value) = source.get_property_str(def.name)
                && let Some(next) = result.with_property_str(def.name, value)
            {
                result = next;
            }
        }
        result
    }
    fn get_post_process_pos(self, _level: &dyn BlockGetter, pos: BlockPos) -> Option<BlockPos> {
        match self.block() {
            Block::BROWN_MUSHROOM | Block::RED_MUSHROOM => Some(pos),
            Block::SOUL_SAND | Block::MAGMA_BLOCK => Some(pos.above()),
            _ => None,
        }
    }
}

/// `SupportType.isSupporting(state, level, pos, direction)`.
pub fn is_supporting(state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction, support: SupportType) -> bool {
    let shape = state.get_block_support_shape(level, pos);
    match support {
        SupportType::Full => is_face_full(&shape, direction),
        SupportType::Center => {
            // `CENTER_SUPPORT_SHAPE` = box(7, 0, 7, 9, 10, 9) face test.
            let center = crate::world::level::shapes::column(2.0, 0.0, 10.0);
            !mc_core::phys::shapes::Shapes::join_is_not_empty(
                &shape.get_face_shape(direction),
                &center,
                mc_core::phys::shapes::BooleanOp::ONLY_SECOND,
            )
        }
        SupportType::Rigid => {
            // `RIGID_SUPPORT_SHAPE` = block minus box(2, 0, 2, 14, 16, 14).
            let rigid = mc_core::phys::shapes::Shapes::join(
                &mc_core::phys::shapes::Shapes::block(),
                &crate::world::level::shapes::box_(2.0, 0.0, 2.0, 14.0, 16.0, 14.0),
                mc_core::phys::shapes::BooleanOp::ONLY_FIRST,
            );
            !mc_core::phys::shapes::Shapes::join_is_not_empty(&shape.get_face_shape(direction), &rigid, mc_core::phys::shapes::BooleanOp::ONLY_SECOND)
        }
    }
}
