//! `net.minecraft.world.level.material.FlowingFluid`: the spreading algorithm shared by water and lava.
//!
//! Vanilla's `OCCLUSION_CACHE` (a bounded cache of `canPassThroughWall` results) is a pure memoisation of a function of
//! `(source state, target state, direction)` and is not reproduced; `SpreadContext`'s per-call caches are (they are
//! keyed by the position relative to the spread origin exactly like vanilla).

use super::{FluidBehaviour, FluidStateExt, fluid_behaviour};
use crate::server::level::ServerLevel;
use crate::world::level::block::{BlockStateExt, behaviour, helpers};
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::phys::Vec3;
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::{Fluid, FluidState};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::LazyLock;

/// The abstract members of `FlowingFluid` (`WaterFluid` / `LavaFluid` implement them).
pub trait FlowingFluid: FluidBehaviour {
    /// `getFlowing()`.
    fn get_flowing_fluid(&self) -> Fluid;
    /// `getSource()`.
    fn get_source_fluid(&self) -> Fluid;
    /// `canConvertToSource(level)`.
    fn can_convert_to_source(&self, level: &ServerLevel) -> bool;
    /// `beforeDestroyingBlock(level, pos, state)`.
    fn before_destroying_block(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState);
    /// `getSlopeFindDistance(level)`.
    fn get_slope_find_distance(&self, level: &dyn LevelReader) -> i32;
    /// `getDropOff(level)`.
    fn get_drop_off(&self, level: &dyn LevelReader) -> i32;
    /// `getSpreadDelay(level, pos, oldFluidState, newFluidState)`.
    fn get_spread_delay(
        &self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        _old: FluidState,
        _new: FluidState,
    ) -> i32 {
        self.get_tick_delay(&*level)
    }
    /// `spreadTo(level, pos, state, direction, target)`.
    fn spread_to(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
        _direction: Direction,
        target: FluidState,
    ) {
        default_spread_to(self, level, pos, state, target);
    }
}

// ---------------------------------------------------------------------------------------------- state lookup

/// `(fluid, falling, amount)` -> state; amount 0 = the source state (`getSource(falling)`).
static STATE_TABLE: LazyLock<HashMap<(Fluid, bool, u8), FluidState>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for fluid in Fluid::iter() {
        for state in fluid.states() {
            let key = if state.is_source() { 0 } else { state.amount() };
            map.insert((fluid, is_falling(state), key), state);
        }
    }
    map
});

/// `getFlowing(amount, falling)` of the flowing fluid `flowing`.
pub fn flowing_state(flowing: Fluid, amount: i32, falling: bool) -> FluidState {
    STATE_TABLE[&(flowing, falling, amount as u8)]
}

/// `getSource(falling)` of the source fluid `source`.
pub fn source_state(source: Fluid, falling: bool) -> FluidState {
    STATE_TABLE[&(source, falling, 0)]
}

/// `fluidState.getValue(FALLING)`.
pub fn is_falling(state: FluidState) -> bool {
    state
        .data()
        .properties
        .iter()
        .any(|&(k, v)| k == "falling" && v == "true")
}

/// `FlowingFluid.getLegacyLevel(fluidState)`.
pub fn get_legacy_level(state: FluidState) -> i32 {
    if state.is_source() {
        0
    } else {
        8 - i32::from(state.amount()).min(8) + if is_falling(state) { 8 } else { 0 }
    }
}

// ------------------------------------------------------------------------------------------------ flow / height

fn affects_flow<F: FlowingFluid + ?Sized>(f: &F, neighbour: FluidState) -> bool {
    neighbour.is_empty() || f.is_same(neighbour.get_type())
}

fn is_solid_face<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &dyn BlockGetter,
    pos: BlockPos,
    direction: Direction,
) -> bool {
    let state = level.get_block_state(pos);
    let fluid_state = level.get_fluid_state(pos);
    if f.is_same(fluid_state.get_type()) {
        false
    } else if direction == Direction::Up {
        true
    } else if helpers::is_instance_of(state, "IceBlock") {
        false
    } else {
        state.is_face_sturdy_at(level, pos, direction)
    }
}

/// `getFlow(level, pos, fluidState)`.
pub fn get_flow<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &dyn BlockGetter,
    pos: BlockPos,
    fluid_state: FluidState,
) -> Vec3 {
    let mut flow_x = 0.0f64;
    let mut flow_z = 0.0f64;
    for direction in Direction::HORIZONTAL {
        let block_pos = pos.relative(direction);
        let neighbour = level.get_fluid_state(block_pos);
        if affects_flow(f, neighbour) {
            let mut neighbour_height = neighbour.own_height();
            let mut distance = 0.0f32;
            if neighbour_height == 0.0 {
                if !level.get_block_state(block_pos).is_tag("blocks_fluid_flow") {
                    let below = block_pos.below();
                    let below_neighbour = level.get_fluid_state(below);
                    if affects_flow(f, below_neighbour) {
                        neighbour_height = below_neighbour.own_height();
                        if neighbour_height > 0.0 {
                            distance = fluid_state.own_height() - (neighbour_height - 0.8888889);
                        }
                    }
                }
            } else if neighbour_height > 0.0 {
                distance = fluid_state.own_height() - neighbour_height;
            }
            if distance != 0.0 {
                flow_x += f64::from(direction.get_step_x() as f32 * distance);
                flow_z += f64::from(direction.get_step_z() as f32 * distance);
            }
        }
    }
    let mut flow = Vec3::new(flow_x, 0.0, flow_z);
    if is_falling(fluid_state) {
        for direction in Direction::HORIZONTAL {
            let block_pos = pos.relative(direction);
            if is_solid_face(f, level, block_pos, direction)
                || is_solid_face(f, level, block_pos.above(), direction)
            {
                flow = flow.normalize().add(Vec3::new(0.0, -6.0, 0.0));
                break;
            }
        }
    }
    flow.normalize()
}

fn has_same_above(fluid_state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
    fluid_behaviour(fluid_state.get_type()).is_same(level.get_fluid_state(pos.above()).get_type())
}

/// `getHeight(fluidState, level, pos)`.
pub fn get_height(fluid_state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> f32 {
    if has_same_above(fluid_state, level, pos) {
        1.0
    } else {
        fluid_state.own_height()
    }
}

/// `getShape(state, level, pos)`, including vanilla's per-state `shapes` memo: the first height computed for a fluid state
/// stays (a source block with water above and one without share it, so the answer depends on which position was asked first).
/// The memo lives per game thread: a server has one game thread, and vanilla's memo is per JVM, so in-process runs that
/// replay several worlds on separate threads each start with an empty one like a fresh vanilla JVM.
pub fn get_shape(state: FluidState, level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
    if state.amount() == 9 && has_same_above(state, level, pos) {
        return Shapes::block();
    }
    thread_local! {
        static SHAPES: RefCell<Vec<Option<VoxelShape>>> = RefCell::new(vec![None; usize::from(u8::MAX) + 1]);
    }
    let index = state.id() as usize;
    if let Some(shape) = SHAPES.with_borrow(|shapes| shapes[index].clone()) {
        return shape;
    }
    let shape = Shapes::box_(
        0.0,
        0.0,
        0.0,
        1.0,
        f64::from(state.get_height(level, pos)),
        1.0,
    );
    SHAPES.with_borrow_mut(|shapes| shapes[index] = Some(shape.clone()));
    shape
}

// --------------------------------------------------------------------------------------------- pass-through logic

/// `canPassThroughWall(direction, level, sourcePos, sourceState, targetPos, targetState)`.
fn can_pass_through_wall(
    direction: Direction,
    level: &dyn BlockGetter,
    source_pos: BlockPos,
    source_state: BlockState,
    target_pos: BlockPos,
    target_state: BlockState,
) -> bool {
    let target_shape = target_state.get_collision_shape(level, target_pos);
    if target_shape.ptr_eq(&Shapes::block()) {
        return false;
    }
    let source_shape = source_state.get_collision_shape(level, source_pos);
    if source_shape.ptr_eq(&Shapes::block()) {
        return false;
    }
    if source_shape.ptr_eq(&Shapes::empty()) && target_shape.ptr_eq(&Shapes::empty()) {
        return true;
    }
    !Shapes::merged_face_occludes(&source_shape, &target_shape, direction)
}

fn can_hold_any_fluid(state: BlockState) -> bool {
    behaviour(state.block()).is_liquid_block_container() || state.is_tag("washed_away_by_fluids")
}

fn can_hold_specific_fluid(
    level: &dyn BlockGetter,
    pos: BlockPos,
    state: BlockState,
    new_fluid: Fluid,
) -> bool {
    let b = behaviour(state.block());
    !b.is_liquid_block_container() || b.can_place_liquid(None, level, pos, state, new_fluid)
}

fn can_hold_fluid(
    level: &dyn BlockGetter,
    pos: BlockPos,
    state: BlockState,
    new_fluid: Fluid,
) -> bool {
    can_hold_any_fluid(state) && can_hold_specific_fluid(level, pos, state, new_fluid)
}

fn is_source_block_of_this_type<F: FlowingFluid + ?Sized>(f: &F, state: FluidState) -> bool {
    f.is_same(state.get_type()) && state.is_source()
}

fn is_water_hole<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &dyn BlockGetter,
    top_pos: BlockPos,
    top_state: BlockState,
    bottom_pos: BlockPos,
    bottom_state: BlockState,
) -> bool {
    if !can_pass_through_wall(
        Direction::Down,
        level,
        top_pos,
        top_state,
        bottom_pos,
        bottom_state,
    ) {
        false
    } else if f.is_same(bottom_state.get_fluid_state().get_type()) {
        true
    } else {
        can_hold_fluid(level, bottom_pos, bottom_state, f.get_flowing_fluid())
    }
}

fn can_maybe_pass_through<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &dyn BlockGetter,
    source_pos: BlockPos,
    source_state: BlockState,
    direction: Direction,
    test_pos: BlockPos,
    test_state: BlockState,
    test_fluid_state: FluidState,
) -> bool {
    !is_source_block_of_this_type(f, test_fluid_state)
        && can_hold_any_fluid(test_state)
        && can_pass_through_wall(
            direction,
            level,
            source_pos,
            source_state,
            test_pos,
            test_state,
        )
}

fn can_pass_through<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &dyn BlockGetter,
    fluid: Fluid,
    source_pos: BlockPos,
    source_state: BlockState,
    direction: Direction,
    test_pos: BlockPos,
    test_state: BlockState,
    test_fluid_state: FluidState,
) -> bool {
    can_maybe_pass_through(
        f,
        level,
        source_pos,
        source_state,
        direction,
        test_pos,
        test_state,
        test_fluid_state,
    ) && can_hold_specific_fluid(level, test_pos, test_state, fluid)
}

fn source_neighbor_count<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &dyn BlockGetter,
    pos: BlockPos,
) -> i32 {
    Direction::HORIZONTAL
        .iter()
        .filter(|&&d| is_source_block_of_this_type(f, level.get_fluid_state(pos.relative(d))))
        .count() as i32
}

// -------------------------------------------------------------------------------------------------- spreading

/// `FlowingFluid.SpreadContext`: per-`getSpread` caches keyed by the (x, z) offset to the origin.
struct SpreadContext<'a> {
    level: &'a dyn BlockGetter,
    origin: BlockPos,
    state_cache: HashMap<i16, BlockState>,
    hole_cache: HashMap<i16, bool>,
}

impl<'a> SpreadContext<'a> {
    fn new(level: &'a dyn BlockGetter, origin: BlockPos) -> Self {
        Self {
            level,
            origin,
            state_cache: HashMap::new(),
            hole_cache: HashMap::new(),
        }
    }

    fn cache_key(&self, pos: BlockPos) -> i16 {
        let relative_x = pos.x().wrapping_sub(self.origin.x());
        let relative_z = pos.z().wrapping_sub(self.origin.z());
        ((((relative_x.wrapping_add(128)) & 0xFF) << 8) | (relative_z.wrapping_add(128) & 0xFF))
            as i16
    }

    fn get_block_state(&mut self, pos: BlockPos) -> BlockState {
        let key = self.cache_key(pos);
        self.block_state_keyed(pos, key)
    }

    fn block_state_keyed(&mut self, pos: BlockPos, key: i16) -> BlockState {
        let level = self.level;
        *self
            .state_cache
            .entry(key)
            .or_insert_with(|| level.get_block_state(pos))
    }

    fn is_hole<F: FlowingFluid + ?Sized>(&mut self, f: &F, pos: BlockPos) -> bool {
        let key = self.cache_key(pos);
        if let Some(&hole) = self.hole_cache.get(&key) {
            return hole;
        }
        let state = self.block_state_keyed(pos, key);
        let below = pos.below();
        let below_state = self.level.get_block_state(below);
        let hole = is_water_hole(f, self.level, pos, state, below, below_state);
        self.hole_cache.insert(key, hole);
        hole
    }
}

fn get_slope_distance<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &dyn LevelReader,
    pos: BlockPos,
    pass: i32,
    from: Direction,
    state: BlockState,
    context: &mut SpreadContext<'_>,
) -> i32 {
    let mut lowest = 1000;
    for direction in Direction::HORIZONTAL {
        if direction == from {
            continue;
        }
        let test_pos = pos.relative(direction);
        let test_state = context.get_block_state(test_pos);
        let test_fluid_state = test_state.get_fluid_state();
        if can_pass_through(
            f,
            level,
            f.get_flowing_fluid(),
            pos,
            state,
            direction,
            test_pos,
            test_state,
            test_fluid_state,
        ) {
            if context.is_hole(f, test_pos) {
                return pass;
            }
            if pass < f.get_slope_find_distance(level) {
                let v = get_slope_distance(
                    f,
                    level,
                    test_pos,
                    pass + 1,
                    direction.get_opposite(),
                    test_state,
                    context,
                );
                if v < lowest {
                    lowest = v;
                }
            }
        }
    }
    lowest
}

/// `getSpread(level, pos, state)`: the sides the fluid flows to, in `EnumMap<Direction, _>` (ordinal) order.
fn get_spread<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &ServerLevel,
    pos: BlockPos,
    state: BlockState,
) -> Vec<(Direction, FluidState)> {
    let mut lowest = 1000;
    let mut result: Vec<(Direction, FluidState)> = Vec::new();
    let mut context: Option<SpreadContext<'_>> = None;
    for direction in Direction::HORIZONTAL {
        let test_pos = pos.relative(direction);
        let test_state = BlockGetter::get_block_state(level, test_pos);
        let test_fluid_state = test_state.get_fluid_state();
        if can_maybe_pass_through(
            f,
            level,
            pos,
            state,
            direction,
            test_pos,
            test_state,
            test_fluid_state,
        ) {
            let new_fluid = get_new_liquid(f, level, test_pos, test_state);
            if can_hold_specific_fluid(level, test_pos, test_state, new_fluid.get_type()) {
                let context = context.get_or_insert_with(|| SpreadContext::new(level, pos));
                let distance = if context.is_hole(f, test_pos) {
                    0
                } else {
                    get_slope_distance(
                        f,
                        level,
                        test_pos,
                        1,
                        direction.get_opposite(),
                        test_state,
                        context,
                    )
                };
                if distance < lowest {
                    result.clear();
                }
                if distance <= lowest {
                    if test_fluid_state.can_be_replaced_with(
                        level,
                        test_pos,
                        new_fluid.get_type(),
                        direction,
                    ) {
                        result.push((direction, new_fluid));
                    }
                    lowest = distance;
                }
            }
        }
    }
    result.sort_by_key(|(d, _)| d.ordinal());
    result
}

/// `getNewLiquid(level, pos, state)`.
pub fn get_new_liquid<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &ServerLevel,
    pos: BlockPos,
    state: BlockState,
) -> FluidState {
    let mut highest_neighbor = 0;
    let mut neighbour_sources = 0;
    for direction in Direction::HORIZONTAL {
        let relative_pos = pos.relative(direction);
        let block_state = BlockGetter::get_block_state(level, relative_pos);
        let fluid_state = block_state.get_fluid_state();
        if f.is_same(fluid_state.get_type())
            && can_pass_through_wall(direction, level, pos, state, relative_pos, block_state)
        {
            if fluid_state.is_source() {
                neighbour_sources += 1;
            }
            highest_neighbor = highest_neighbor.max(i32::from(fluid_state.amount()));
        }
    }
    if neighbour_sources >= 2 && f.can_convert_to_source(level) {
        let below_state = BlockGetter::get_block_state(level, pos.relative(Direction::Down));
        let below_fluid = below_state.get_fluid_state();
        if below_state.is_solid() || is_source_block_of_this_type(f, below_fluid) {
            return source_state(f.get_source_fluid(), false);
        }
    }
    let above_pos = pos.relative(Direction::Up);
    let above_state = BlockGetter::get_block_state(level, above_pos);
    let above_fluid = above_state.get_fluid_state();
    if !above_fluid.is_empty()
        && f.is_same(above_fluid.get_type())
        && can_pass_through_wall(Direction::Up, level, pos, state, above_pos, above_state)
    {
        flowing_state(f.get_flowing_fluid(), 8, true)
    } else {
        let amount = highest_neighbor - f.get_drop_off(level);
        if amount <= 0 {
            Fluid::EMPTY.default_state()
        } else {
            flowing_state(f.get_flowing_fluid(), amount, false)
        }
    }
}

/// The base `spreadTo`: containers take the liquid, anything else is replaced (after `beforeDestroyingBlock`).
pub fn default_spread_to<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    target: FluidState,
) {
    let b = behaviour(state.block());
    if b.is_liquid_block_container() {
        b.place_liquid(level, pos, state, target);
    } else {
        if !state.is_air() {
            f.before_destroying_block(level, pos, state);
        }
        LevelAccessor::set_block_and_update(level, pos, target.create_legacy_block());
    }
}

fn spread_to_sides<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &mut ServerLevel,
    pos: BlockPos,
    fluid_state: FluidState,
    state: BlockState,
) {
    let mut neighbor = i32::from(fluid_state.amount()) - f.get_drop_off(&*level);
    if is_falling(fluid_state) {
        neighbor = 7;
    }
    if neighbor > 0 {
        let spreads = get_spread(f, level, pos, state);
        for (direction, new_neighbor_fluid) in spreads {
            let neighbor_pos = pos.relative(direction);
            let neighbor_state = BlockGetter::get_block_state(level, neighbor_pos);
            f.spread_to(
                level,
                neighbor_pos,
                neighbor_state,
                direction,
                new_neighbor_fluid,
            );
        }
    }
}

fn spread<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    fluid_state: FluidState,
) {
    if fluid_state.is_empty() {
        return;
    }
    let below_pos = pos.below();
    let below_state = BlockGetter::get_block_state(level, below_pos);
    let below_fluid = below_state.get_fluid_state();
    if can_maybe_pass_through(
        f,
        level,
        pos,
        state,
        Direction::Down,
        below_pos,
        below_state,
        below_fluid,
    ) {
        let new_below_fluid = get_new_liquid(f, level, below_pos, below_state);
        let new_below_type = new_below_fluid.get_type();
        if below_fluid.can_be_replaced_with(level, below_pos, new_below_type, Direction::Down)
            && can_hold_specific_fluid(level, below_pos, below_state, new_below_type)
        {
            f.spread_to(
                level,
                below_pos,
                below_state,
                Direction::Down,
                new_below_fluid,
            );
            if source_neighbor_count(f, level, pos) >= 3 {
                spread_to_sides(f, level, pos, fluid_state, state);
            }
            return;
        }
    }
    if fluid_state.is_source() || !is_water_hole(f, level, pos, state, below_pos, below_state) {
        spread_to_sides(f, level, pos, fluid_state, state);
    }
}

/// `tick(level, pos, blockState, fluidState)`.
pub fn tick<F: FlowingFluid + ?Sized>(
    f: &F,
    level: &mut ServerLevel,
    pos: BlockPos,
    mut block_state: BlockState,
    mut fluid_state: FluidState,
) {
    if !fluid_state.is_source() {
        let current = BlockGetter::get_block_state(level, pos);
        let new_fluid_state = get_new_liquid(f, level, pos, current);
        let tick_delay = f.get_spread_delay(level, pos, fluid_state, new_fluid_state);
        if new_fluid_state.is_empty() {
            fluid_state = new_fluid_state;
            block_state = helpers::air();
            LevelAccessor::set_block_and_update(level, pos, block_state);
        } else if new_fluid_state != fluid_state {
            fluid_state = new_fluid_state;
            block_state = new_fluid_state.create_legacy_block();
            LevelAccessor::set_block_and_update(level, pos, block_state);
            level.schedule_fluid_tick(
                pos,
                new_fluid_state.get_type(),
                tick_delay,
                mc_world::ticks::TickPriority::Normal,
            );
        }
    }
    spread(f, level, pos, block_state, fluid_state);
}
