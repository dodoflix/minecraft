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
pub fn get_state_for_placement(block: mc_data::block::Block, level: &mut crate::server::level::ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
    super::behaviour(block).get_state_for_placement(block, level, context)
}

/// `Block.dropResources(state, level, pos, blockEntity, breaker, ItemStack.EMPTY)` of a destroyed block (`Level.destroyBlock`).
pub fn drop_resources(level: &mut crate::server::level::ServerLevel, state: BlockState, pos: BlockPos, breaker: Option<crate::world::entity::EntityId>) {
    super::super::storage::loot::block_drops::drop_resources_for_destroy(level, state, pos, breaker);
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

// ==== [Blocks:TechnicalA] ====
/// `BaseEntityBlock.triggerEvent`: forwards the block event to the block entity at `pos`.
pub fn base_entity_trigger_event(level: &mut crate::server::level::ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool {
    level.with_block_entity(pos, |level, be| be.trigger_event(level, b0, b1)).unwrap_or(false)
}

/// `Block.popResource(level, pos, itemStack)`.
pub fn pop_resource(level: &mut crate::server::level::ServerLevel, pos: BlockPos, stack: crate::world::item::ItemStack) {
    use crate::world::entity::item::ItemEntity;
    let half_height = f64::from(mc_data::EntityType::ITEM.data().height) / 2.0;
    let random = level.random();
    let x = f64::from(pos.x()) + 0.5 + mc_core::math::mth::next_double(random, -0.25, 0.25);
    let y = f64::from(pos.y()) + 0.5 + mc_core::math::mth::next_double(random, -0.25, 0.25) - half_height;
    let z = f64::from(pos.z()) + 0.5 + mc_core::math::mth::next_double(random, -0.25, 0.25);
    pop_resource_entity(level, stack, |level, stack| ItemEntity::with_stack(level, x, y, z, stack));
}

/// `Block.popResourceFromFace(level, pos, face, itemStack)`.
pub fn pop_resource_from_face(level: &mut crate::server::level::ServerLevel, pos: BlockPos, face: Direction, stack: crate::world::item::ItemStack) {
    use crate::world::entity::item::ItemEntity;
    use mc_core::math::mth::next_double;
    let (step_x, step_y, step_z) = (face.get_step_x(), face.get_step_y(), face.get_step_z());
    let half_width = f64::from(mc_data::EntityType::ITEM.data().width) / 2.0;
    let half_height = f64::from(mc_data::EntityType::ITEM.data().height) / 2.0;
    let random = level.random();
    let x = f64::from(pos.x()) + 0.5 + if step_x == 0 { next_double(random, -0.25, 0.25) } else { f64::from(step_x) * (0.5 + half_width) };
    let y = f64::from(pos.y()) + 0.5 + if step_y == 0 { next_double(random, -0.25, 0.25) } else { f64::from(step_y) * (0.5 + half_height) } - half_height;
    let z = f64::from(pos.z()) + 0.5 + if step_z == 0 { next_double(random, -0.25, 0.25) } else { f64::from(step_z) * (0.5 + half_width) };
    let dx = if step_x == 0 { next_double(random, -0.1, 0.1) } else { f64::from(step_x) * 0.1 };
    let dy = if step_y == 0 { next_double(random, 0.0, 0.1) } else { f64::from(step_y) * 0.1 + 0.1 };
    let dz = if step_z == 0 { next_double(random, -0.1, 0.1) } else { f64::from(step_z) * 0.1 };
    pop_resource_entity(level, stack, |level, stack| ItemEntity::with_stack_and_motion(level, x, y, z, stack, dx, dy, dz));
}

/// `Block.popResource(level, entityFactory, itemStack)`: `BLOCK_DROPS` gate, default pickup delay.
fn pop_resource_entity(
    level: &mut crate::server::level::ServerLevel,
    stack: crate::world::item::ItemStack,
    factory: impl FnOnce(&crate::server::level::ServerLevel, crate::world::item::ItemStack) -> crate::world::entity::item::ItemEntity,
) {
    if !stack.is_empty() && level.block_drops() {
        let mut entity = factory(level, stack);
        entity.set_default_pick_up_delay();
        level.add_fresh_entity(Box::new(entity));
    }
}

/// `Block.popExperience(level, pos, amount)`.
pub fn pop_experience(level: &mut crate::server::level::ServerLevel, pos: BlockPos, amount: i32) {
    if level.block_drops() {
        crate::world::entity::experience_orb::ExperienceOrb::award(level, &mc_core::phys::Vec3::at_center_of(&pos), amount);
    }
}
