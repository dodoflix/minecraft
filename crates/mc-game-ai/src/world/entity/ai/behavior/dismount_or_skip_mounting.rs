//! `DismountOrSkipMounting`.

use mc_game_core::world::entity::ai::behavior::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::memory::memory_module_type::RIDE_TARGET;
use mc_game_core::world::entity::ai::sensing::closer_than;
use mc_game_core::world::entity::entity::Entity;

/// `DismountOrSkipMounting.create(maxWalkDistToRideTarget, dontRideIf)`; `dontRideIf(body, vehicle)`.
pub fn create(max_walk_dist_to_ride_target: i32, dont_ride_if: impl Fn(&dyn Entity, &dyn Entity) -> bool + 'static) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let ride_target = i.registered(&RIDE_TARGET);
    i.create(move |c| {
        let current_vehicle = c.body.base().vehicle;
        let target_vehicle = c.try_get(&ride_target).map(|r| r.0);
        let Some(vehicle_id) = current_vehicle.or(target_vehicle) else { return false };
        let keep = c.level.get_entity(vehicle_id).is_some_and(|vehicle| {
            // `isVehicleValid(body, vehicle, maxWalkDistToRideTarget)` (same level always holds)
            vehicle.is_alive() && closer_than(vehicle, &*c.body, f64::from(max_walk_dist_to_ride_target)) && !dont_ride_if(&*c.body, vehicle)
        });
        if keep {
            false
        } else {
            let (level, body) = (&mut *c.level, &mut *c.body);
            level.stop_riding(body);
            c.erase(&ride_target);
            true
        }
    })
}
