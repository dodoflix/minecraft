//! `SetEntityLookTargetSometimes` (deprecated in vanilla, still used by `PiglinAi`/`HoglinAi`-style brains).

use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    LOOK_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use mc_game_core::world::entity::ai::sensing::distance_to_sqr;
use mc_game_core::world::entity::entity::Entity;

/// `SetEntityLookTargetSometimes.Ticker`.
pub struct Ticker {
    min_inclusive: i32,
    max_inclusive: i32,
    ticks_until_next_start: i32,
}

impl Ticker {
    /// `new Ticker(interval)`; `interval` is a `UniformInt`.
    pub fn new(min_inclusive: i32, max_inclusive: i32) -> Self {
        assert!(min_inclusive > 1, "IllegalArgumentException");
        Self {
            min_inclusive,
            max_inclusive,
            ticks_until_next_start: 0,
        }
    }

    /// `tickDownAndCheck(random)`.
    pub fn tick_down_and_check(&mut self, random: &mut dyn RandomSource) -> bool {
        if self.ticks_until_next_start == 0 {
            self.ticks_until_next_start =
                random.next_int_between_inclusive(self.min_inclusive, self.max_inclusive) - 1;
            false
        } else {
            self.ticks_until_next_start -= 1;
            self.ticks_until_next_start == 0
        }
    }
}

/// `create(maxDist, interval)`; `interval` = the `UniformInt` bounds.
pub fn create(max_dist: f32, interval_min_inclusive: i32, interval_max_inclusive: i32) -> OneShot {
    create_with(
        max_dist,
        interval_min_inclusive,
        interval_max_inclusive,
        |_| true,
    )
}

/// `create(type, maxDist, interval)`.
pub fn create_type(
    ty: EntityType,
    max_dist: f32,
    interval_min_inclusive: i32,
    interval_max_inclusive: i32,
) -> OneShot {
    create_with(
        max_dist,
        interval_min_inclusive,
        interval_max_inclusive,
        move |mob| mob.get_type() == ty,
    )
}

fn create_with(
    max_dist: f32,
    interval_min_inclusive: i32,
    interval_max_inclusive: i32,
    predicate: impl Fn(&dyn Entity) -> bool + 'static,
) -> OneShot {
    let max_dist_sqr = max_dist * max_dist;
    let mut ticker = Ticker::new(interval_min_inclusive, interval_max_inclusive);
    let mut i = BehaviorBuilder::instance();
    let look_target = i.absent(&LOOK_TARGET);
    let nearest_entities = i.present(&NEAREST_VISIBLE_LIVING_ENTITIES);
    i.create(move |c| {
        let body: &dyn Entity = &*c.body;
        let target = c
            .get(&nearest_entities)
            .find_closest(&*c.level, body, |mob| {
                predicate(mob) && distance_to_sqr(mob, body) <= f64::from(max_dist_sqr)
            });
        let Some(target) = target else { return false };
        if !ticker.tick_down_and_check(&mut c.level.tick_state.random) {
            return false;
        }
        let Some(tracker) = c
            .level
            .get_entity_or_recently_removed(target.0)
            .map(|e| EntityTracker::new(e, true))
        else {
            return false;
        };
        c.set(&look_target, PositionTracker::Entity(tracker));
        true
    })
}
