//! `PanicGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::mob::navigation;
use crate::world::level::BlockGetter;
use crate::world::level::block::state::BlockStateExt;
use mc_core::pos::BlockPos;
use mc_data::fluid::Fluid;

pub const WATER_CHECK_DISTANCE_VERTICAL: i32 = 1;

/// `PanicGoal` (`panicCausingDamageTypes` = the `damage_type` tag the last damage must be in).
pub struct PanicGoal {
    pub speed_modifier: f64,
    pub pos_x: f64,
    pub pos_y: f64,
    pub pos_z: f64,
    pub is_running: bool,
    /// `panicCausingDamageTypes` (`Function<PathfinderMob, TagKey<DamageType>>`).
    panic_causing_damage_types: PanicTag,
}

#[derive(Clone, Copy)]
enum PanicTag {
    Fixed(&'static str),
    Func(fn(&dyn Entity) -> &'static str),
}

impl PanicGoal {
    /// `PanicGoal(mob, speedModifier)`.
    pub fn new(speed_modifier: f64) -> Self {
        Self::with_tag(
            speed_modifier,
            crate::world::damagesource::DamageTypeTags::PANIC_CAUSES,
        )
    }

    /// `PanicGoal(mob, speedModifier, panicCausingDamageTypes)`.
    pub fn with_tag(speed_modifier: f64, panic_causing_damage_types: &'static str) -> Self {
        Self {
            speed_modifier,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            is_running: false,
            panic_causing_damage_types: PanicTag::Fixed(panic_causing_damage_types),
        }
    }

    /// `PanicGoal(mob, speedModifier, Function<PathfinderMob, TagKey<DamageType>>)` (`PolarBear`).
    pub fn with_tag_fn(
        speed_modifier: f64,
        panic_causing_damage_types: fn(&dyn Entity) -> &'static str,
    ) -> Self {
        Self {
            speed_modifier,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            is_running: false,
            panic_causing_damage_types: PanicTag::Func(panic_causing_damage_types),
        }
    }

    /// `shouldPanic()`.
    pub fn should_panic(&self, mob: &dyn Entity, level: &ServerLevel) -> bool {
        mob.living()
            .and_then(|l| l.last_damage_source.as_ref())
            .is_some_and(|s| {
                s.is_tag(
                    level,
                    match self.panic_causing_damage_types {
                        PanicTag::Fixed(t) => t,
                        PanicTag::Func(f) => f(mob),
                    },
                )
            })
    }

    /// `findRandomPosition()`.
    pub fn find_random_position(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        match default_random_pos::get_pos(mob, level, 5, 4) {
            None => false,
            Some(pos) => {
                self.pos_x = pos.x;
                self.pos_y = pos.y;
                self.pos_z = pos.z;
                true
            }
        }
    }

    /// `isRunning()`.
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// `lookForWater(level, mob, xzDist)`.
    pub fn look_for_water(level: &ServerLevel, mob: &dyn Entity, xz_dist: i32) -> Option<BlockPos> {
        let mob_position = mob.block_position();
        if !level
            .get_block_state(mob_position)
            .get_collision_shape(level, mob_position)
            .is_empty()
        {
            return None;
        }
        let water = mc_data::tags::get::<Fluid>("water");
        BlockPos::within_box_by_manhattan_distance(
            &mob_position,
            xz_dist,
            WATER_CHECK_DISTANCE_VERTICAL,
            xz_dist,
        )
        .find(|p| water.is_some_and(|t| t.contains(level.get_fluid_state(*p).fluid())))
    }

    /// The goal's `canUse` body, reusable by subclasses (`TamableAnimalPanicGoal`, ...).
    pub fn can_use_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let should_panic = self.should_panic(mob, level);
        self.can_use_with(mob, level, should_panic, 5)
    }

    /// `canUse()` with `shouldPanic()` supplied by the subclass (`MountPanicGoal`, `FoxPanicGoal`: `!x && super.shouldPanic()`)
    /// and the `lookForWater` range (`TurtlePanicGoal`: 7).
    pub fn can_use_with(
        &mut self,
        mob: &mut dyn Entity,
        level: &mut ServerLevel,
        should_panic: bool,
        water_range: i32,
    ) -> bool {
        if !should_panic {
            return false;
        }
        if mob.is_on_fire() {
            if let Some(pos) = Self::look_for_water(level, mob, water_range) {
                self.pos_x = f64::from(pos.x());
                self.pos_y = f64::from(pos.y());
                self.pos_z = f64::from(pos.z());
                return true;
            }
        }
        self.find_random_position(mob, level)
    }

    pub fn start_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to(
            mob,
            level,
            self.pos_x,
            self.pos_y,
            self.pos_z,
            self.speed_modifier,
        );
        self.is_running = true;
    }
}

impl Goal for PanicGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.start_impl(mob, level);
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.is_running = false;
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !navigation::is_done(mob, level)
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }

    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        Some(self)
    }
}
