//! `InsideBlockEffectType` / `InsideBlockEffectApplier` (+ `StepBasedCollector`) and the `Entity.checkInsideBlocks` /
//! `applyEffectsFromBlocks` machinery that feeds `Block.entityInside` / `Fluid.entityInside`.

use super::entity::{Entity, Movement};
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::collision::for_each_block_intersected_between;
use crate::world::level::material::{FluidStateExt, fluid_behaviour};
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::phys::shapes::Shapes;
use mc_core::pos::BlockPos;
use std::collections::HashSet;

/// `InsideBlockEffectType` (declaration order = `StepBasedCollector.APPLY_ORDER`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InsideBlockEffectType {
    Freeze,
    ClearFreeze,
    FireIgnite,
    LavaIgnite,
    Extinguish,
}

impl InsideBlockEffectType {
    pub const VALUES: [InsideBlockEffectType; 5] = [
        Self::Freeze,
        Self::ClearFreeze,
        Self::FireIgnite,
        Self::LavaIgnite,
        Self::Extinguish,
    ];

    /// `type.effect().accept(entity)`.
    pub fn apply_effect(self, entity: &mut dyn Entity, level: &mut ServerLevel) {
        match self {
            Self::Freeze => {
                entity.set_is_in_powder_snow(true);
                if entity.can_freeze() {
                    let ticks = entity
                        .get_ticks_required_to_freeze()
                        .min(entity.base().get_ticks_frozen().wrapping_add(1));
                    entity.base_mut().set_ticks_frozen(ticks);
                }
            }
            Self::ClearFreeze => entity.clear_freeze(),
            Self::FireIgnite => fire_ignite(entity, level),
            Self::LavaIgnite => entity.lava_ignite(),
            Self::Extinguish => entity.clear_fire(),
        }
    }
}

/// `BaseFireBlock.fireIgnite(entity)`.
pub fn fire_ignite(entity: &mut dyn Entity, level: &mut ServerLevel) {
    use mc_core::random::RandomSource;
    if entity.fire_immune() {
        return;
    }
    if entity.get_remaining_fire_ticks() < 0 {
        let t = entity.get_remaining_fire_ticks() + 1;
        entity.set_remaining_fire_ticks(t);
    } else if entity.is_player() {
        let added_fire_ticks =
            crate::world::level::LevelAccessor::random(level).next_int_between(1, 3);
        let t = entity
            .get_remaining_fire_ticks()
            .wrapping_add(added_fire_ticks);
        entity.set_remaining_fire_ticks(t);
    }
    if entity.get_remaining_fire_ticks() >= 0 {
        entity.ignite_for_seconds(8.0);
    }
}

/// A `Consumer<Entity>` of the collector (the level is what `entity.level()` is in vanilla).
pub type EntityEffect = Box<dyn FnOnce(&mut dyn Entity, &mut ServerLevel)>;

/// `InsideBlockEffectApplier`.
pub trait InsideBlockEffectApplier {
    fn apply(&mut self, effect: InsideBlockEffectType);
    fn run_before(&mut self, effect: InsideBlockEffectType, consumer: EntityEffect);
    fn run_after(&mut self, effect: InsideBlockEffectType, consumer: EntityEffect);
}

/// `InsideBlockEffectApplier.NOOP`.
pub struct NoopApplier;

impl InsideBlockEffectApplier for NoopApplier {
    fn apply(&mut self, _effect: InsideBlockEffectType) {}
    fn run_before(&mut self, _effect: InsideBlockEffectType, _consumer: EntityEffect) {}
    fn run_after(&mut self, _effect: InsideBlockEffectType, _consumer: EntityEffect) {}
}

const NO_STEP: i32 = -1;

/// `InsideBlockEffectApplier.StepBasedCollector`.
pub struct StepBasedCollector {
    effects_in_step: [bool; 5],
    before_effects_in_step: [Vec<EntityEffect>; 5],
    after_effects_in_step: [Vec<EntityEffect>; 5],
    final_effects: Vec<EntityEffect>,
    last_step: i32,
}

impl Default for StepBasedCollector {
    fn default() -> Self {
        Self {
            effects_in_step: [false; 5],
            before_effects_in_step: Default::default(),
            after_effects_in_step: Default::default(),
            final_effects: Vec::new(),
            last_step: NO_STEP,
        }
    }
}

impl StepBasedCollector {
    pub fn advance_step(&mut self, step: i32) {
        if self.last_step != step {
            self.last_step = step;
            self.flush_step();
        }
    }

    /// `applyAndClear(entity)`.
    pub fn apply_and_clear(&mut self, entity: &mut dyn Entity, level: &mut ServerLevel) {
        self.flush_step();
        for effect in std::mem::take(&mut self.final_effects) {
            if !entity.is_alive() {
                break;
            }
            effect(entity, level);
        }
        self.final_effects.clear();
        self.last_step = NO_STEP;
    }

    fn flush_step(&mut self) {
        for (i, effect_type) in InsideBlockEffectType::VALUES.into_iter().enumerate() {
            self.final_effects
                .append(&mut self.before_effects_in_step[i]);
            if std::mem::take(&mut self.effects_in_step[i]) {
                self.final_effects.push(Box::new(move |entity, level| {
                    effect_type.apply_effect(entity, level)
                }));
            }
            self.final_effects
                .append(&mut self.after_effects_in_step[i]);
        }
    }
}

impl InsideBlockEffectApplier for StepBasedCollector {
    fn apply(&mut self, effect: InsideBlockEffectType) {
        self.effects_in_step[effect as usize] = true;
    }

    fn run_before(&mut self, effect: InsideBlockEffectType, consumer: EntityEffect) {
        self.before_effects_in_step[effect as usize].push(consumer);
    }

    fn run_after(&mut self, effect: InsideBlockEffectType, consumer: EntityEffect) {
        self.after_effects_in_step[effect as usize].push(consumer);
    }
}

/// `Entity.collidedWithShapeMovingFrom(from, to, aabbs)`.
pub fn collided_with_shape_moving_from(
    entity: &dyn Entity,
    from: Vec3,
    to: Vec3,
    aabbs: &[mc_core::phys::AABB],
) -> bool {
    let bounding_box_at_from = entity.make_bounding_box(from);
    bounding_box_at_from.collided_along_vector(to.subtract(from), aabbs)
}

/// `Entity.collidedWithFluid(fluidState, blockPos, from, to)`.
pub fn collided_with_fluid(
    entity: &dyn Entity,
    level: &ServerLevel,
    fluid_state: mc_data::fluid::FluidState,
    pos: BlockPos,
    from: Vec3,
    to: Vec3,
) -> bool {
    match fluid_state.get_aabb(level, pos) {
        Some(aabb) => collided_with_shape_moving_from(entity, from, to, &[aabb]),
        None => false,
    }
}

/// `Entity.checkInsideBlocks(from, to, collector, visitedBlocks, maxMovementIterations)`: returns the iterations used.
fn check_inside_blocks_step(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    from: Vec3,
    to: Vec3,
    collector: &mut StepBasedCollector,
    visited_blocks: &mut HashSet<i64>,
    max_movement_iterations: i32,
) -> i32 {
    let deflated_bounding_box_at_target = entity
        .make_bounding_box(to)
        .deflate_scalar(f64::from(1.0E-5f32));
    let moved_far = from.distance_to_sqr(to) > mth::square_f64(0.9999900000002526);
    let mut iterations = 0;
    let target_box = deflated_bounding_box_at_target;
    for_each_block_intersected_between(
        from,
        to,
        &target_box,
        &mut |block_intersection: BlockPos, iteration: i32| {
            if !entity.is_alive() {
                return false;
            }
            if iteration >= max_movement_iterations {
                return false;
            }
            iterations = iteration;
            let state = level.get_block_state(block_intersection);
            if state.is_air() {
                return true;
            }
            let intersect_shape =
                state.get_entity_inside_collision_shape(&*level, block_intersection, &*entity);
            let inside_block = intersect_shape.ptr_eq(&Shapes::block())
                || collided_with_shape_moving_from(
                    &*entity,
                    from,
                    to,
                    &intersect_shape
                        .move_vec(Vec3::new(
                            f64::from(block_intersection.x()),
                            f64::from(block_intersection.y()),
                            f64::from(block_intersection.z()),
                        ))
                        .to_aabbs(),
                );
            let fluid_state = state.get_fluid_state();
            let inside_fluid =
                collided_with_fluid(&*entity, level, fluid_state, block_intersection, from, to);
            if (inside_block || inside_fluid) && visited_blocks.insert(block_intersection.as_long())
            {
                if inside_block {
                    let is_precise =
                        moved_far || block_intersects_box(&target_box, block_intersection);
                    collector.advance_step(iteration);
                    state.entity_inside(level, block_intersection, entity, collector, is_precise);
                    entity.on_inside_block(level, state);
                }
                if inside_fluid {
                    collector.advance_step(iteration);
                    fluid_behaviour(fluid_state.get_type()).entity_inside(
                        level,
                        block_intersection,
                        entity,
                        collector,
                    );
                }
            }
            true
        },
    );
    iterations + 1
}

/// `AABB.intersects(BlockPos)` (`intersects(x, y, z, x + 1, y + 1, z + 1)`).
fn block_intersects_box(aabb: &mc_core::phys::AABB, pos: BlockPos) -> bool {
    aabb.intersects_coords(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
        f64::from(pos.x() + 1),
        f64::from(pos.y() + 1),
        f64::from(pos.z() + 1),
    )
}

/// `Entity.checkInsideBlocks(movements, collector)`.
fn check_inside_blocks(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    movements: &[Movement],
    collector: &mut StepBasedCollector,
) {
    if !entity.is_affected_by_blocks() {
        return;
    }
    let mut visited_blocks: HashSet<i64> = HashSet::new();
    for movement in movements {
        let mut pos = movement.from;
        let delta = movement.to.subtract(movement.from);
        let mut max_movement_iterations = 16;
        match movement.axis_dependent_original_movement {
            Some(original) if delta.length_sqr() > 0.0 => {
                for axis in Direction::axis_step_order(original) {
                    let axis_move = delta.get(axis);
                    if axis_move != 0.0 {
                        let to = pos.relative(
                            Direction::from_axis_and_direction(
                                axis,
                                mc_core::direction::AxisDirection::Positive,
                            ),
                            axis_move,
                        );
                        max_movement_iterations -= check_inside_blocks_step(
                            entity,
                            level,
                            pos,
                            to,
                            collector,
                            &mut visited_blocks,
                            max_movement_iterations,
                        );
                        pos = to;
                    }
                }
            }
            _ => {
                max_movement_iterations -= check_inside_blocks_step(
                    entity,
                    level,
                    movement.from,
                    movement.to,
                    collector,
                    &mut visited_blocks,
                    16,
                );
            }
        }
        if max_movement_iterations <= 0 {
            check_inside_blocks_step(
                entity,
                level,
                movement.to,
                movement.to,
                collector,
                &mut visited_blocks,
                1,
            );
        }
    }
    visited_blocks.clear();
}

/// `Entity.applyEffectsFromBlocks(movements)`.
pub fn apply_effects_from_blocks_for(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    movements: &[Movement],
) {
    if !entity.is_affected_by_blocks() {
        return;
    }
    if entity.base().on_ground() {
        let effect_pos = super::movement::get_on_pos_legacy(entity, level);
        let effect_state = level.get_block_state(effect_pos);
        effect_state.step_on(level, effect_pos, entity);
    }
    let was_on_fire = entity.is_on_fire();
    let was_freezing = entity.is_freezing();
    let previous_remaining_fire_ticks = entity.get_remaining_fire_ticks();
    let mut collector = std::mem::take(&mut entity.base_mut().inside_effect_collector);
    check_inside_blocks(entity, level, movements, &mut collector);
    collector.apply_and_clear(entity, level);
    entity.base_mut().inside_effect_collector = collector;
    if entity.is_in_rain(level) {
        entity.clear_fire();
    }
    if (was_on_fire && !entity.is_on_fire()) || (was_freezing && !entity.is_freezing()) {
        entity.play_entity_on_fire_extinguished_sound(level);
    }
    let was_ignited_this_tick = entity.get_remaining_fire_ticks() > previous_remaining_fire_ticks;
    if !entity.is_on_fire() && !was_ignited_this_tick {
        let t = -entity.get_fire_immune_ticks();
        entity.set_remaining_fire_ticks(t);
    }
}

/// `Entity.applyEffectsFromBlocks()`: consumes `movementThisTick`.
pub fn apply_effects_from_blocks(entity: &mut dyn Entity, level: &mut ServerLevel) {
    let b = entity.base_mut();
    b.final_movements_this_tick.clear();
    b.final_movements_this_tick
        .extend(b.movement_this_tick.drain(..));
    match b.final_movements_this_tick.last().copied() {
        None => {
            let m = Movement {
                from: b.old_position(),
                to: b.position(),
                axis_dependent_original_movement: None,
            };
            b.final_movements_this_tick.push(m);
        }
        Some(last) if last.to.distance_to_sqr(b.position()) > f64::from(9.9999994E-11f32) => {
            let m = Movement {
                from: last.to,
                to: b.position(),
                axis_dependent_original_movement: None,
            };
            b.final_movements_this_tick.push(m);
        }
        Some(_) => {}
    }
    let movements = b.final_movements_this_tick.clone();
    apply_effects_from_blocks_for(entity, level, &movements);
}

/// `Entity.applyEffectsFromBlocksForLastMovements()`.
pub fn apply_effects_from_blocks_for_last_movements(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
) {
    let movements = entity.base().final_movements_this_tick.clone();
    apply_effects_from_blocks_for(entity, level, &movements);
}

/// `Entity.applyEffectsFromBlocks(from, to)`.
pub fn apply_effects_from_blocks_between(
    entity: &mut dyn Entity,
    level: &mut ServerLevel,
    from: Vec3,
    to: Vec3,
) {
    apply_effects_from_blocks_for(
        entity,
        level,
        &[Movement {
            from,
            to,
            axis_dependent_original_movement: None,
        }],
    );
}
