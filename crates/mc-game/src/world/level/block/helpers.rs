//! Static helpers of `net.minecraft.world.level.block.Block`.

use super::BlockStateExt;
use super::state::UPDATE_SHAPE_ORDER;
use crate::world::level::LevelAccessor;
use crate::world::level::flags::{UPDATE_SUPPRESS_DROPS};
use crate::world::level::shapes::is_shape_full_block;
use mc_core::direction::Direction;
use mc_core::phys::shapes::VoxelShape;
use mc_core::pos::BlockPos;
use mc_data::block::{BlockState, SupportType};

pub use crate::world::level::shapes::{box_ as box_shape, is_face_full};

/// `Block.isShapeFullBlock`.
pub fn is_shape_full(shape: &VoxelShape) -> bool {
    is_shape_full_block(shape)
}

/// `Block.updateFromNeighbourShapes`.
pub fn update_from_neighbour_shapes(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) -> BlockState {
    let mut new_state = state;
    for direction in UPDATE_SHAPE_ORDER {
        let neighbour_pos = pos.relative(direction);
        let neighbour_state = level.get_block_state(neighbour_pos);
        new_state = new_state.update_shape(level, pos, direction, neighbour_pos, neighbour_state);
    }
    new_state
}

/// `Block.updateOrDestroy(old, new, level, pos, flags)` (limit 512).
pub fn update_or_destroy(block_state: BlockState, new_state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, update_flags: u32) {
    update_or_destroy_limited(block_state, new_state, level, pos, update_flags, 512);
}

/// `Block.updateOrDestroy(old, new, level, pos, flags, updateLimit)`.
pub fn update_or_destroy_limited(
    block_state: BlockState,
    new_state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    update_flags: u32,
    update_limit: i32,
) {
    if new_state != block_state {
        if new_state.is_air() {
            level.destroy_block(pos, update_flags & UPDATE_SUPPRESS_DROPS == 0, None, update_limit);
        } else {
            level.set_block(pos, new_state, update_flags & !UPDATE_SUPPRESS_DROPS, update_limit);
        }
    }
}

/// `Block.canSupportRigidBlock(level, below)`.
pub fn can_support_rigid_block(level: &dyn crate::world::level::BlockGetter, below: BlockPos) -> bool {
    level.get_block_state(below).is_face_sturdy_support(level, below, Direction::Up, SupportType::Rigid)
}

/// `Block.canSupportCenter(level, belowPos, direction)`.
pub fn can_support_center(level: &dyn crate::world::level::BlockGetter, below_pos: BlockPos, direction: Direction) -> bool {
    let state = level.get_block_state(below_pos);
    if direction == Direction::Down && state.is_tag("unstable_bottom_center") {
        false
    } else {
        state.is_face_sturdy_support(level, below_pos, direction, SupportType::Center)
    }
}

/// `Block.getStateForPlacement(context)` of `block`.
pub fn get_state_for_placement(block: mc_data::block::Block, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
    super::behaviour(block).get_state_for_placement(block, context)
}

/// Implementation of `Block.dropResources(state, level, pos, blockEntity, breaker, tool)` supplied by the loot slice
/// (loot tables, item entities). Until it registers itself nothing drops.
pub type DropResourcesFn = fn(&mut crate::server::level::ServerLevel, BlockState, BlockPos, Option<crate::world::entity::EntityId>);

static DROP_RESOURCES: std::sync::OnceLock<DropResourcesFn> = std::sync::OnceLock::new();

/// Registers the drop implementation (once, at startup).
pub fn set_drop_resources(f: DropResourcesFn) {
    let _ = DROP_RESOURCES.set(f);
}

/// `Block.dropResources(state, level, pos, blockEntity, breaker, ItemStack.EMPTY)`.
pub fn drop_resources(level: &mut crate::server::level::ServerLevel, state: BlockState, pos: BlockPos, breaker: Option<crate::world::entity::EntityId>) {
    if let Some(f) = DROP_RESOURCES.get() {
        f(level, state, pos, breaker);
    }
}

/// `Block.pushEntitiesUp(state, newState, level, pos)`: entities in the way of a block that grows are lifted.
pub fn push_entities_up(level: &mut crate::server::level::ServerLevel, state: BlockState, new_state: BlockState, pos: BlockPos) -> BlockState {
    use mc_core::phys::Vec3;
    use mc_core::phys::shapes::{BooleanOp, Shapes};
    let offset_shape = Shapes::join_unoptimized(&state.get_collision_shape(level, pos), &new_state.get_collision_shape(level, pos), BooleanOp::ONLY_SECOND).move_vec3i(*pos);
    if offset_shape.is_empty() {
        return new_state;
    }
    for id in level.get_entities(None, &offset_shape.bounds(), |_| true) {
        level.with_entity(id, |_, entity| {
            let offset = Shapes::collide(mc_core::direction::Axis::Y, &entity.get_bounding_box().move_(0.0, 1.0, 0.0), [&offset_shape], -1.0);
            // `teleportRelative(0, 1 + offset, 0)`
            let target = entity.position().add(Vec3::new(0.0, 1.0 + offset, 0.0));
            entity.set_pos_vec(target);
        });
    }
    new_state
}

/// `Blocks.AIR.defaultBlockState()`.
pub fn air() -> BlockState {
    mc_data::block::Block::AIR.default_state()
}

/// `state.getBlock() instanceof <java class>` for a Java class (not interface) simple name.
pub fn is_instance_of(state: BlockState, class: &str) -> bool {
    state.block().data().classes.contains(&class)
}

/// `Block.isExceptionForConnection(state)`.
pub fn is_exception_for_connection(state: BlockState) -> bool {
    use mc_data::block::Block;
    is_instance_of(state, "LeavesBlock")
        || state.is_block(Block::BARRIER)
        || state.is_block(Block::CARVED_PUMPKIN)
        || state.is_block(Block::JACK_O_LANTERN)
        || state.is_block(Block::MELON)
        || state.is_block(Block::PUMPKIN)
        || state.is_tag("shulker_boxes")
}

/// `LevelReader.isWaterAt(pos)`.
pub fn is_water_at(level: &dyn crate::world::level::LevelReader, pos: BlockPos) -> bool {
    use crate::world::level::material::FluidStateExt;
    level.get_fluid_state(pos).is_tag("water")
}

/// `ticks.scheduleTick(pos, Fluids.WATER, Fluids.WATER.getTickDelay(level))`.
pub fn schedule_water_tick(level: &mut dyn LevelAccessor, pos: BlockPos) {
    schedule_fluid_tick(level, pos, mc_data::fluid::Fluid::WATER);
}

/// `ticks.scheduleTick(pos, fluid, fluid.getTickDelay(level))` (priority NORMAL).
pub fn schedule_fluid_tick(level: &mut dyn LevelAccessor, pos: BlockPos, fluid: mc_data::fluid::Fluid) {
    let delay = crate::world::level::material::fluid_behaviour(fluid).get_tick_delay(&*level);
    level.schedule_fluid_tick(pos, fluid, delay, mc_world::ticks::TickPriority::Normal);
}

/// `ticks.scheduleTick(pos, block, delay)` (priority NORMAL).
pub fn schedule_block_tick(level: &mut dyn LevelAccessor, pos: BlockPos, block: mc_data::block::Block, delay: i32) {
    level.schedule_block_tick(pos, block, delay, mc_world::ticks::TickPriority::Normal);
}

/// `state.setValue(FACING, rotation.rotate(state.getValue(FACING)))`.
pub fn rotate_facing(state: BlockState, facing: mc_data::props::EnumProperty<Direction>, rotation: mc_core::rotation::Rotation) -> BlockState {
    state.set(facing, rotation.rotate(state.get(facing)))
}

/// `state.rotate(mirror.getRotation(state.getValue(FACING)))`.
pub fn mirror_facing(state: BlockState, facing: mc_data::props::EnumProperty<Direction>, mirror: mc_core::mirror::Mirror) -> BlockState {
    state.rotate(mirror.get_rotation(state.get(facing)))
}

/// `PipeBlock.PROPERTY_BY_DIRECTION`: the boolean `north`/`east`/`south`/`west`/`up`/`down` property of a side.
pub fn pipe_property(direction: Direction) -> mc_data::props::BooleanProperty {
    use mc_data::props;
    match direction {
        Direction::North => props::NORTH,
        Direction::East => props::EAST,
        Direction::South => props::SOUTH,
        Direction::West => props::WEST,
        Direction::Up => props::UP,
        Direction::Down => props::DOWN,
    }
}

/// The `rotate` shared by `CrossCollisionBlock`, `WallBlock`, `RedStoneWireBlock`, `TripWireBlock`, `VineBlock`: the
/// values of the four horizontal side properties `[north, east, south, west]` move around.
pub fn rotate_sides<P: mc_data::props::Property>(state: BlockState, rotation: mc_core::rotation::Rotation, p: [P; 4]) -> BlockState {
    use mc_core::rotation::Rotation;
    let [n, e, s, w] = p;
    let (vn, ve, vs, vw) = (state.get(n), state.get(e), state.get(s), state.get(w));
    match rotation {
        Rotation::Clockwise180 => state.set(n, vs).set(e, vw).set(s, vn).set(w, ve),
        Rotation::Counterclockwise90 => state.set(n, ve).set(e, vs).set(s, vw).set(w, vn),
        Rotation::Clockwise90 => state.set(n, vw).set(e, vn).set(s, ve).set(w, vs),
        Rotation::None => state,
    }
}

/// The `mirror` shared by the same classes (`default -> super.mirror` is the identity for them).
pub fn mirror_sides<P: mc_data::props::Property>(state: BlockState, mirror: mc_core::mirror::Mirror, p: [P; 4]) -> BlockState {
    use mc_core::mirror::Mirror;
    let [n, e, s, w] = p;
    match mirror {
        Mirror::LeftRight => state.set(n, state.get(s)).set(s, state.get(n)),
        Mirror::FrontBack => state.set(e, state.get(w)).set(w, state.get(e)),
        Mirror::None => state,
    }
}
