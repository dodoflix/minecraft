//! `AnimalPanic`.

use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::fluid::Fluid;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::damagesource::DamageTypeTags;
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    HURT_BY, IS_PANICKING, WALK_TARGET,
};
use mc_game_core::world::entity::ai::util::{air_and_water_random_pos, land_random_pos};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::mob::navigation;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::block::state::BlockStateExt;

pub const PANIC_MIN_DURATION: i32 = 100;
pub const PANIC_MAX_DURATION: i32 = 120;
const PANIC_DISTANCE_HORIZONTAL: i32 = 5;
const PANIC_DISTANCE_VERTICAL: i32 = 4;

type DamageTypeGetter = Box<dyn Fn(&dyn Entity) -> &'static str>;
type PanicPositionGetter = Box<dyn FnMut(&mut ServerLevel, &mut dyn Entity) -> Option<Vec3>>;

pub struct AnimalPanic {
    speed_multiplier: f32,
    panic_causing_damage_types: DamageTypeGetter,
    position_getter: PanicPositionGetter,
}

impl AnimalPanic {
    /// `new AnimalPanic(speedMultiplier)`.
    pub fn new(speed_multiplier: f32) -> BehaviorBase<AnimalPanic> {
        Self::with_all(
            speed_multiplier,
            |_| DamageTypeTags::PANIC_CAUSES,
            |level, mob| {
                land_random_pos::get_pos(
                    mob,
                    level,
                    PANIC_DISTANCE_HORIZONTAL,
                    PANIC_DISTANCE_VERTICAL,
                )
            },
        )
    }

    /// `new AnimalPanic(speedMultiplier, flyHeight)`.
    pub fn flying(speed_multiplier: f32, fly_height: i32) -> BehaviorBase<AnimalPanic> {
        Self::with_all(
            speed_multiplier,
            |_| DamageTypeTags::PANIC_CAUSES,
            move |level, mob| {
                let view = mob.get_view_vector(0.0);
                air_and_water_random_pos::get_pos(
                    mob,
                    level,
                    PANIC_DISTANCE_HORIZONTAL,
                    PANIC_DISTANCE_VERTICAL,
                    fly_height,
                    view.x,
                    view.z,
                    f64::from(std::f32::consts::FRAC_PI_2),
                )
            },
        )
    }

    /// `new AnimalPanic(speedMultiplier, panicCausingDamageTypes)`.
    pub fn with_damage_types(
        speed_multiplier: f32,
        panic_causing_damage_types: impl Fn(&dyn Entity) -> &'static str + 'static,
    ) -> BehaviorBase<AnimalPanic> {
        Self::with_all(
            speed_multiplier,
            panic_causing_damage_types,
            |level, mob| {
                land_random_pos::get_pos(
                    mob,
                    level,
                    PANIC_DISTANCE_HORIZONTAL,
                    PANIC_DISTANCE_VERTICAL,
                )
            },
        )
    }

    /// `new AnimalPanic(speedMultiplier, panicCausingDamageTypes, positionGetter)`.
    pub fn with_all(
        speed_multiplier: f32,
        panic_causing_damage_types: impl Fn(&dyn Entity) -> &'static str + 'static,
        position_getter: impl FnMut(&mut ServerLevel, &mut dyn Entity) -> Option<Vec3> + 'static,
    ) -> BehaviorBase<AnimalPanic> {
        BehaviorBase::with_duration(
            entry_condition![IS_PANICKING => Registered, HURT_BY => Registered],
            PANIC_MIN_DURATION,
            PANIC_MAX_DURATION,
            AnimalPanic {
                speed_multiplier,
                panic_causing_damage_types: Box::new(panic_causing_damage_types),
                position_getter: Box::new(position_getter),
            },
        )
    }

    fn get_panic_pos(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) -> Option<Vec3> {
        if body.is_on_fire()
            && let Some(nearest_water) =
                look_for_water(level, &*body).map(|p| Vec3::at_bottom_center_of(&p))
        {
            return Some(nearest_water);
        }
        (self.position_getter)(level, body)
    }
}

fn is_water(state: mc_data::block::BlockState) -> bool {
    mc_data::tags::get::<Fluid>("water")
        .is_some_and(|t| t.contains(state.get_fluid_state().fluid()))
}

/// `lookForWater(level, mob)`.
fn look_for_water(level: &ServerLevel, mob: &dyn Entity) -> Option<BlockPos> {
    let mob_position = mob.block_position();
    if !level
        .get_block_state(mob_position)
        .get_collision_shape(level, mob_position)
        .is_empty()
    {
        return None;
    }
    let two_wide = mth::ceil_f32(mob.base().get_bb_width()) == 2;
    BlockPos::within_box_by_manhattan_distance(&mob_position, 5, 1, 5).find(|&pos| {
        is_water(level.get_block_state(pos))
            && (!two_wide
                || (is_water(level.get_block_state(pos.south()))
                    && is_water(level.get_block_state(pos.east()))
                    && is_water(level.get_block_state(pos.south().east()))))
    })
}

impl Behavior for AnimalPanic {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let tag = (self.panic_causing_damage_types)(&*body);
        body.brain()
            .get_memory(&HURT_BY)
            .is_some_and(|d| d.0.is_tag(level, tag))
            || body.brain().has_memory_value(IS_PANICKING)
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        _body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        true
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().set_memory(&IS_PANICKING, true);
        body.brain_mut().erase_memory(&WALK_TARGET);
        navigation::stop(body, level);
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&IS_PANICKING);
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if navigation::is_done(body, level)
            && let Some(panic_to_pos) = self.get_panic_pos(level, body)
        {
            body.brain_mut().set_memory(
                &WALK_TARGET,
                WalkTarget::from_vec3(panic_to_pos, self.speed_multiplier, 0),
            );
        }
    }
}
