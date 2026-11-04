//! Riding: `Entity.startRiding/stopRiding/removeVehicle/ejectPassengers/addPassenger/removePassenger`,
//! `positionRider`, `rideTick` and `ServerLevel.tickPassenger`.
//!
//! Vehicle/passenger links are ids. The acting entity is passed as `&mut dyn Entity` (it is taken out of the level);
//! the other side of the link is reached through the level's entity manager (deferred when that one is taken out too).
//! The `ClientboundSetPassengers` packet is produced by the tracker's `sendChanges` when the passenger list changed.

use super::ServerLevel;
use crate::world::entity::entity::Entity;
use crate::world::entity::{EntityAttachment, EntityId};
use mc_core::phys::Vec3;
use mc_data::enums::Pose;

impl ServerLevel {
    /// `Entity.startRiding(vehicle, force, sendEventAndTriggers)`.
    pub fn start_riding(&mut self, rider: &mut dyn Entity, vehicle_id: EntityId, force: bool, _send_event: bool) -> bool {
        if rider.base().vehicle == Some(vehicle_id) {
            return false;
        }
        let Some(vehicle) = self.entity_manager.get_any(vehicle_id) else { return false };
        if !vehicle.could_accept_passenger() || !vehicle.get_type().data().serializable {
            return false;
        }
        // a vehicle that (indirectly) rides the rider would make a cycle
        let mut cur = vehicle.base().vehicle;
        while let Some(v) = cur {
            if v == rider.id() {
                return false;
            }
            cur = self.entity_manager.get_any(v).and_then(|e| e.base().vehicle);
        }
        if !(force || (rider.can_ride(vehicle) && vehicle.can_add_passenger(rider.as_dyn()))) {
            return false;
        }
        if rider.base().is_passenger() {
            self.stop_riding(rider);
        }
        rider.set_pose(Pose::Standing);
        rider.base_mut().vehicle = Some(vehicle_id);
        let rider_id = rider.id();
        let rider_is_player = rider.is_player();
        let first_is_player = self.entity_manager.get_any(vehicle_id).and_then(|v| v.base().passengers.first().copied()).and_then(|p| self.entity_manager.get_any(p)).is_some_and(|p| p.is_player());
        if let Some(v) = self.entity_manager.get_any_mut(vehicle_id) {
            let passengers = &mut v.base_mut().passengers;
            if passengers.is_empty() {
                passengers.push(rider_id);
            } else if rider_is_player && !first_is_player {
                passengers.insert(0, rider_id);
            } else {
                passengers.push(rider_id);
            }
        }
        true
    }

    /// `Entity.removeVehicle()` / `stopRiding()`.
    pub fn stop_riding(&mut self, rider: &mut dyn Entity) {
        let Some(vehicle_id) = rider.base_mut().vehicle.take() else { return };
        rider.base_mut().boarding_cooldown = entity_boarding_cooldown();
        let rider_id = rider.id();
        self.entity_manager.defer_or_apply(
            vehicle_id,
            Box::new(move |v: &mut dyn Entity| {
                v.base_mut().passengers.retain(|&p| p != rider_id);
            }),
        );
    }

    /// `Entity.ejectPassengers()`: `vehicle` is the (taken out) vehicle; passengers are stopped in reverse order.
    pub fn eject_passengers(&mut self, vehicle: &mut dyn Entity) {
        let vehicle_id = vehicle.id();
        for pid in vehicle.base().passengers.clone().into_iter().rev() {
            match self.take_entity(pid) {
                Some(mut p) => {
                    p.base_mut().vehicle = None;
                    p.base_mut().boarding_cooldown = entity_boarding_cooldown();
                    self.put_back_entity(pid, p);
                }
                None => {
                    // the passenger is out too: drop its link when it comes back
                    self.entity_manager.defer_or_apply(pid, Box::new(move |p: &mut dyn Entity| {
                        if p.base().vehicle == Some(vehicle_id) {
                            p.base_mut().vehicle = None;
                            p.base_mut().boarding_cooldown = entity_boarding_cooldown();
                        }
                    }));
                }
            }
        }
        vehicle.base_mut().passengers.clear();
    }

    /// `Entity.unRide()`.
    pub fn un_ride(&mut self, entity: &mut dyn Entity) {
        if entity.base().is_vehicle() {
            self.eject_passengers(entity);
        }
        if entity.base().is_passenger() {
            self.stop_riding(entity);
        }
    }

    /// `Entity.remove(reason)` including the riding part of `setRemoved`: a destroyed entity leaves its vehicle and
    /// every passenger leaves it.
    pub fn remove_entity(&mut self, id: EntityId, reason: crate::world::entity::RemovalReason) {
        self.with_entity(id, |level, e| {
            if reason.should_destroy() {
                level.stop_riding(e);
            }
            level.eject_passengers(e);
            e.remove(reason);
        });
    }

    /// `ServerLevel.tickPassenger(vehicle, passenger)`.
    pub fn tick_passenger(&mut self, vehicle: &mut dyn Entity, passenger_id: EntityId) {
        let Some(mut passenger) = self.take_entity(passenger_id) else { return };
        if passenger.is_removed() || passenger.base().vehicle != Some(vehicle.id()) {
            self.stop_riding(passenger.as_dyn_mut());
            // the vehicle (out of storage) still lists it
            vehicle.base_mut().passengers.retain(|&p| p != passenger_id);
        } else if passenger.is_player() || self.entity_manager.tick_list.contains(passenger_id) {
            crate::world::entity::entity::common_tick(passenger.as_dyn_mut());
            self.ride_tick(passenger.as_dyn_mut(), vehicle);
            for sub in passenger.base().passengers.clone() {
                self.tick_passenger(passenger.as_dyn_mut(), sub);
            }
        }
        self.put_back_entity(passenger_id, passenger);
    }

    /// `Entity.rideTick()`: `passenger` rides `vehicle` (both taken out).
    pub fn ride_tick(&mut self, passenger: &mut dyn Entity, vehicle: &mut dyn Entity) {
        passenger.base_mut().set_delta_movement(Vec3::ZERO);
        passenger.tick(self);
        if passenger.base().is_passenger() {
            position_rider(vehicle, passenger);
        }
    }
}

fn entity_boarding_cooldown() -> i32 {
    crate::world::entity::entity::BOARDING_COOLDOWN
}

/// `Entity.positionRider(passenger)` with `setPos`.
pub fn position_rider(vehicle: &dyn Entity, passenger: &mut dyn Entity) {
    if !vehicle.base().passengers.contains(&passenger.id()) {
        return;
    }
    let pos = get_passenger_riding_position(vehicle, passenger);
    let offset = passenger.get_attachments().get(EntityAttachment::Vehicle, 0, passenger.base().y_rot());
    passenger.set_pos(pos.x - offset.x, pos.y - offset.y, pos.z - offset.z);
}

/// `getPassengerRidingPosition(passenger)` (default passenger attachment).
pub fn get_passenger_riding_position(vehicle: &dyn Entity, passenger: &dyn Entity) -> Vec3 {
    let index = vehicle.base().passengers.iter().position(|&p| p == passenger.id()).map_or(-1, |i| i as i32);
    let attach = vehicle.get_attachments().get_clamped(EntityAttachment::Passenger, index, vehicle.base().y_rot());
    vehicle.base().position().add(attach)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::plain::PlainEntity;
    use mc_data::EntityType;

    fn setup() -> (ServerLevel, EntityId, EntityId) {
        let mut level = ServerLevel::detached(mc_core::ident::Identifier::parse("minecraft:overworld").unwrap());
        let mut boat = PlainEntity::new(EntityType::OAK_BOAT, level.next_entity_id());
        boat.set_pos(10.0, 64.0, 10.0);
        let boat_id = boat.id();
        level.add_fresh_entity(Box::new(boat));
        let rider = PlainEntity::new(EntityType::PIG, level.next_entity_id());
        let rider_id = rider.id();
        level.add_fresh_entity(Box::new(rider));
        (level, boat_id, rider_id)
    }

    #[test]
    fn start_riding_links_both_sides_and_stop_riding_unlinks() {
        let (mut level, boat, rider) = setup();
        let mut r = level.take_entity(rider).unwrap();
        assert!(level.start_riding(r.as_dyn_mut(), boat, false, true));
        assert_eq!(r.base().vehicle, Some(boat));
        assert!(!level.start_riding(r.as_dyn_mut(), boat, false, true), "already riding it");
        assert_eq!(level.entity_manager.get_any(boat).unwrap().base().passengers, vec![rider]);
        let v = level.entity_manager.get_any(boat).unwrap();
        let pos = get_passenger_riding_position(v, r.as_ref());
        assert_eq!(pos, v.base().position().add(v.get_attachments().get_clamped(EntityAttachment::Passenger, 0, 0.0)));
        level.stop_riding(r.as_dyn_mut());
        assert!(r.base().vehicle.is_none());
        assert_eq!(r.base().boarding_cooldown, 60);
        assert!(level.entity_manager.get_any(boat).unwrap().base().passengers.is_empty());
        level.put_back_entity(rider, r);
    }

    #[test]
    fn cycles_are_rejected() {
        let (mut level, boat, rider) = setup();
        let mut r = level.take_entity(rider).unwrap();
        assert!(level.start_riding(r.as_dyn_mut(), boat, false, true));
        level.put_back_entity(rider, r);
        let mut b = level.take_entity(boat).unwrap();
        assert!(!level.start_riding(b.as_dyn_mut(), rider, true, true));
        level.eject_passengers(b.as_dyn_mut());
        assert!(level.entity_manager.get_any(rider).unwrap().base().vehicle.is_none());
        level.put_back_entity(boat, b);
    }
}
