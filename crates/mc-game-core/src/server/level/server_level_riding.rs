//! Riding: `Entity.startRiding/stopRiding/removeVehicle/ejectPassengers`, `LivingEntity.stopRiding/dismountVehicle`,
//! `Player.removeVehicle`, `ServerLevel.tickNonPassenger/tickPassenger` and `Entity.rideTick`.
//!
//! Vehicle/passenger links are ids. The acting entity is passed as `&mut dyn Entity` (it is taken out of the level);
//! the other side of the link is reached through the level's entity manager (deferred when that one is taken out too).
//! `ServerLevel.tickNonPassenger` puts the vehicle back into the level's storage before its passengers tick, so a
//! passenger finds its vehicle through the level like vanilla's `getVehicle()`.
//! The `ClientboundSetPassengersPacket` is produced by the tracker's `sendChanges` when the passenger list changed.

use super::ServerLevel;
use super::level_tick::LevelTickEnv;
use crate::advancements::triggers::CriteriaTriggers;
use crate::world::entity::EntityId;
use crate::world::entity::ServerPlayer;
use crate::world::entity::entity::{BOARDING_COOLDOWN, Entity};
use crate::world::level::BlockGetter;
use crate::world::level::collision::CollisionGetter;
use mc_core::phys::shapes::Shapes;
use mc_core::phys::{AABB, Vec3};
use mc_data::GameEvent;
use mc_data::enums::Pose;

impl ServerLevel {
    /// `Entity.startRiding(vehicle, force, sendEventAndTriggers)`.
    pub fn start_riding(
        &mut self,
        rider: &mut dyn Entity,
        vehicle_id: EntityId,
        force: bool,
        send_event_and_triggers: bool,
    ) -> bool {
        let vehicle_id = rider.adjust_vehicle_to_ride(self, vehicle_id);
        self.start_riding_impl(rider, vehicle_id, None, force, send_event_and_triggers)
    }

    /// `startRiding` where the vehicle is at hand (taken out of the level, e.g. while its own `interact` runs).
    pub fn start_riding_held(
        &mut self,
        rider: &mut dyn Entity,
        vehicle: &mut dyn Entity,
        force: bool,
        send_event_and_triggers: bool,
    ) -> bool {
        let vehicle_id = vehicle.id();
        self.start_riding_impl(
            rider,
            vehicle_id,
            Some(vehicle),
            force,
            send_event_and_triggers,
        )
    }

    fn start_riding_impl(
        &mut self,
        rider: &mut dyn Entity,
        vehicle_id: EntityId,
        mut held_vehicle: Option<&mut dyn Entity>,
        force: bool,
        send_event_and_triggers: bool,
    ) -> bool {
        rider.before_start_riding();
        if rider.base().vehicle == Some(vehicle_id) {
            return false;
        }
        {
            let vehicle: &dyn Entity = match held_vehicle.as_deref() {
                Some(v) => v,
                None => match self.entity_manager.get_any(vehicle_id) {
                    Some(v) => v,
                    None => return false,
                },
            };
            if !vehicle.could_accept_passenger() || !vehicle.get_type().data().serializable {
                return false;
            }
            // a vehicle that (indirectly) rides the rider would make a cycle
            let mut cur = vehicle.base().vehicle;
            while let Some(v) = cur {
                if v == rider.id() {
                    return false;
                }
                cur = self
                    .entity_manager
                    .get_any(v)
                    .and_then(|e| e.base().vehicle);
            }
            if !(force || (rider.can_ride(vehicle) && vehicle.can_add_passenger(rider.as_dyn()))) {
                return false;
            }
        }
        if rider.base().is_passenger() {
            self.stop_riding(rider);
        }
        rider.set_pose(Pose::Standing);
        rider.base_mut().vehicle = Some(vehicle_id);
        let first_passenger = match held_vehicle.as_deref() {
            Some(v) => v.base().passengers.first().copied(),
            None => self
                .entity_manager
                .get_any(vehicle_id)
                .and_then(|v| v.base().passengers.first().copied()),
        };
        let first_is_player = first_passenger
            .and_then(|p| self.entity_manager.get_any(p))
            .is_some_and(|p| p.is_player());
        let rider_authoritative = rider.is_client_authoritative();
        let rider_id = rider.id();
        let (position, passengers) = match held_vehicle.as_deref_mut() {
            Some(v) => {
                v.add_passenger(&*rider, first_is_player);
                v.after_add_passenger(rider);
                if v.base().passengers.first() == Some(&rider_id) {
                    v.base_mut().first_passenger_client_authoritative = rider_authoritative;
                }
                (v.base().position(), v.base().passengers.clone())
            }
            None => {
                let Some(v) = self.entity_manager.get_any_mut(vehicle_id) else {
                    return false;
                };
                v.add_passenger(&*rider, first_is_player);
                v.after_add_passenger(rider);
                if v.base().passengers.first() == Some(&rider_id) {
                    v.base_mut().first_passenger_client_authoritative = rider_authoritative;
                }
                (v.base().position(), v.base().passengers.clone())
            }
        };
        if send_event_and_triggers {
            self.game_event_from_entity(Some(rider_id), GameEvent::ENTITY_MOUNT, position);
            self.trigger_start_riding(rider, &passengers);
        }
        // `Mob.startRiding`: a leashed mob drops its leash when it starts riding
        if rider.mob().is_some() && crate::world::entity::mob::leashable::is_leashed(rider) {
            crate::world::entity::mob::leashable::drop_leash(rider, self);
        }
        rider.on_vehicle_started(self, vehicle_id, held_vehicle.as_deref());
        true
    }

    /// `LivingEntity.stopRiding()` / `Entity.stopRiding()`: leave the vehicle; living entities are moved to a safe
    /// dismount location.
    pub fn stop_riding(&mut self, rider: &mut dyn Entity) {
        self.stop_riding_from(rider, None);
    }

    /// `Entity.removeVehicle()` without `LivingEntity.stopRiding`'s dismount relocation (`ServerPlayer.teleport`).
    pub fn remove_vehicle(&mut self, rider: &mut dyn Entity) {
        self.remove_vehicle_from(rider, None);
    }

    /// `stopRiding()` where the vehicle may be at hand (taken out of the level) instead of in storage.
    fn stop_riding_from(
        &mut self,
        rider: &mut dyn Entity,
        mut held_vehicle: Option<&mut dyn Entity>,
    ) {
        let Some(old_vehicle) = rider.base().vehicle else {
            return;
        };
        self.remove_vehicle_from(rider, held_vehicle.as_deref_mut());
        if rider.living().is_some() {
            self.dismount_vehicle(rider, old_vehicle, held_vehicle.as_deref());
        }
    }

    /// `Entity.removeVehicle()` (+ `Player.removeVehicle`, `ServerPlayer.removeVehicle` through
    /// [`Entity::on_vehicle_removed`]): clears the link on both sides and plays the dismount game event.
    fn remove_vehicle_from(
        &mut self,
        rider: &mut dyn Entity,
        mut held_vehicle: Option<&mut dyn Entity>,
    ) {
        let Some(old_vehicle_id) = rider.base_mut().vehicle.take() else {
            return;
        };
        let rider_id = rider.id();
        let vehicle_position = match held_vehicle.as_deref_mut() {
            Some(v) => {
                v.remove_passenger(rider_id);
                v.on_passenger_removed(self, rider_id);
                v.refresh_first_passenger_authority(self);
                Some(v.base().position())
            }
            None => {
                self.entity_manager.defer_or_apply(
                    old_vehicle_id,
                    Box::new(move |v: &mut dyn Entity| v.remove_passenger(rider_id)),
                );
                self.with_entity(old_vehicle_id, |level, v| {
                    v.on_passenger_removed(level, rider_id);
                    v.refresh_first_passenger_authority(level);
                });
                self.entity_manager
                    .get_any(old_vehicle_id)
                    .map(|v| v.base().position())
            }
        };
        rider.base_mut().boarding_cooldown = BOARDING_COOLDOWN;
        if rider
            .base()
            .removal_reason()
            .is_none_or(|r| r.should_destroy())
            && let Some(p) = vehicle_position
        {
            self.game_event_from_entity(Some(rider_id), GameEvent::ENTITY_DISMOUNT, p);
        }
        if rider.is_player() {
            rider.base_mut().boarding_cooldown = 0;
        }
        rider.on_vehicle_removed(self, old_vehicle_id, held_vehicle.as_deref());
    }

    /// `LivingEntity.dismountVehicle(vehicle)`; `held_vehicle` is the vehicle when it is out of storage.
    fn dismount_vehicle(
        &mut self,
        rider: &mut dyn Entity,
        vehicle_id: EntityId,
        held_vehicle: Option<&dyn Entity>,
    ) {
        let target = {
            let vehicle: Option<&dyn Entity> =
                held_vehicle.or_else(|| self.entity_manager.get_any(vehicle_id));
            let base = rider.base();
            if rider.is_removed() {
                base.position()
            } else if let Some(v) = vehicle.filter(|v| {
                !v.is_removed()
                    && !self
                        .get_block_state(v.base().block_position())
                        .block()
                        .is_in_tag("portals")
            }) {
                v.get_dismount_location_for_passenger_setting_pose(self, &mut *rider)
            } else {
                let max_y = base.y().max(vehicle.map_or(base.y(), |v| v.base().y()));
                let mut target = Vec3::new(base.x(), max_y, base.z());
                let (width, height) = (base.get_bb_width(), base.get_bb_height());
                if width <= 4.0 && height <= 4.0 {
                    let half_height = f64::from(height) / 2.0;
                    let center = target.add(Vec3::new(0.0, half_height, 0.0));
                    let allowed_centers = Shapes::create(AABB::of_size(
                        center,
                        f64::from(width),
                        f64::from(height),
                        f64::from(width),
                    ));
                    if let Some(free) = self.find_free_position_for(
                        rider.as_dyn(),
                        &allowed_centers,
                        center,
                        f64::from(width),
                        f64::from(height),
                        f64::from(width),
                    ) {
                        target = free.add(Vec3::new(0.0, -half_height, 0.0));
                    }
                }
                target
            }
        };
        rider.dismount_to(self, target.x, target.y, target.z);
    }

    /// `Level.findFreePosition(entity, allowedCenters, preferredCenter, sizeX, sizeY, sizeZ)`.
    pub fn find_free_position_for(
        &self,
        entity: &dyn Entity,
        allowed_centers: &mc_core::phys::shapes::VoxelShape,
        preferred_center: Vec3,
        size_x: f64,
        size_y: f64,
        size_z: f64,
    ) -> Option<Vec3> {
        let context = crate::world::entity::movement::collision_context(entity);
        let border = self.world_border_ref();
        CollisionGetter::find_free_position(
            self,
            Some(&context),
            &|aabb: &AABB| border.is_none_or(|b| b.is_within_bounds_aabb(aabb)),
            allowed_centers,
            preferred_center,
            size_x,
            size_y,
            size_z,
        )
    }

    /// `Entity.ejectPassengers()`: `vehicle` is the (taken out) vehicle; passengers are stopped in reverse order.
    pub fn eject_passengers(&mut self, vehicle: &mut dyn Entity) {
        let vehicle_id = vehicle.id();
        for pid in vehicle.base().passengers.clone().into_iter().rev() {
            match self.take_entity(pid) {
                Some(mut p) => {
                    if p.base().vehicle == Some(vehicle_id) {
                        self.stop_riding_from(p.as_mut(), Some(&mut *vehicle));
                    }
                    self.put_back_entity(pid, p);
                }
                None => {
                    // the passenger is out too: drop its link when it comes back
                    vehicle.remove_passenger(pid);
                    self.entity_manager.defer_or_apply(
                        pid,
                        Box::new(move |p: &mut dyn Entity| {
                            if p.base().vehicle == Some(vehicle_id) {
                                p.base_mut().vehicle = None;
                                p.base_mut().boarding_cooldown =
                                    if p.is_player() { 0 } else { BOARDING_COOLDOWN };
                            }
                        }),
                    );
                }
            }
        }
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
        self.with_entity(id, |level, e| level.remove_held_entity(e, reason));
    }

    /// `remove(reason)` of an entity at hand (taken out of the level): the level-dependent override
    /// ([`Entity::before_remove`]), then `setRemoved`'s riding part, then the entity's own `remove`.
    pub fn remove_held_entity(
        &mut self,
        e: &mut dyn Entity,
        reason: crate::world::entity::RemovalReason,
    ) {
        if e.base().removal_reason().is_none() {
            if reason.should_destroy() {
                e.before_remove(self, reason);
            }
            e.base_mut().set_removal_reason(reason);
        }
        if reason.should_destroy() {
            self.stop_riding(e);
        }
        self.eject_passengers(e);
        e.remove(reason);
        if !e.is_player() {
            self.untrack_discarded_entity(e);
        }
    }

    /// `ServerLevel.tick`'s per-entity step for an entry of the tick list: `checkDespawn`, the riding checks and
    /// `tickNonPassenger` (the vehicle is back in storage while its passengers tick).
    pub fn tick_listed_entity(&mut self, env: &mut dyn LevelTickEnv, id: EntityId) {
        let Some(mut entity) = self.take_entity(id) else {
            return;
        };
        let player_passengers = self
            .entity_manager
            .count_players_among(&entity.base().passengers) as i32;
        if entity.is_removed() || env.is_entity_frozen(&*entity, player_passengers) {
            self.put_back_entity(id, entity);
            return;
        }
        entity.check_despawn(self);
        let mut passengers = Vec::new();
        if entity.is_player()
            || self
                .chunk_source
                .chunk_map
                .distance_manager
                .in_entity_ticking_range(entity.chunk_position().pack())
        {
            if let Some(vehicle_id) = entity.base().vehicle {
                let still_riding = self
                    .entity_manager
                    .get_any(vehicle_id)
                    .is_some_and(|v| !v.is_removed() && v.base().passengers.contains(&id));
                if still_riding {
                    self.put_back_entity(id, entity);
                    return;
                }
                self.stop_riding(entity.as_mut());
            }
            crate::world::entity::entity::common_tick(entity.as_mut());
            entity.refresh_first_passenger_authority(self);
            // a command block minecart runs its command while it ticks, as a rider too
            self.with_command_env(&mut *env, |level| entity.tick(level));
            if self.take_portal_teleport(id) {
                env.teleport_through_portal(self, entity.as_mut());
            }
            passengers = entity.base().passengers.clone();
        }
        self.put_back_entity(id, entity);
        self.with_command_env(&mut *env, |level| {
            for passenger in passengers {
                level.tick_passenger(id, passenger);
            }
        });
    }

    /// `ServerLevel.tickPassenger(vehicle, passenger)` (the vehicle is in storage).
    pub fn tick_passenger(&mut self, vehicle_id: EntityId, passenger_id: EntityId) {
        let Some(mut passenger) = self.take_entity(passenger_id) else {
            return;
        };
        if passenger.is_removed() || passenger.base().vehicle != Some(vehicle_id) {
            self.stop_riding(passenger.as_mut());
            self.put_back_entity(passenger_id, passenger);
            return;
        }
        if passenger.is_player() || self.entity_manager.tick_list.contains(passenger_id) {
            crate::world::entity::entity::common_tick(passenger.as_mut());
            passenger.refresh_first_passenger_authority(self);
            self.ride_tick(passenger.as_mut());
            let sub_passengers = passenger.base().passengers.clone();
            self.put_back_entity(passenger_id, passenger);
            for sub in sub_passengers {
                self.tick_passenger(passenger_id, sub);
            }
        } else {
            self.put_back_entity(passenger_id, passenger);
        }
    }

    /// `Entity.rideTick()` with the overrides of `Player` (a rider that wants to stop riding leaves), `LivingEntity` (the
    /// fall distance is reset) and `ServerPlayer` (the riding statistics of the position change): `passenger` rides the
    /// vehicle it points at (which is in storage).
    pub fn ride_tick(&mut self, passenger: &mut dyn Entity) {
        let before = passenger.base().position();
        // `Player.rideTick`: `wantsToStopRiding` is the shift key
        if passenger.is_player() && passenger.is_shift_key_down() && passenger.base().is_passenger()
        {
            self.stop_riding(passenger);
        } else {
            passenger.base_mut().set_delta_movement(Vec3::ZERO);
            passenger.tick(self);
            if let Some(vehicle_id) = passenger.base().vehicle
                && let Some(vehicle) = self.entity_manager.get_any(vehicle_id)
            {
                vehicle.position_rider(passenger);
            }
            if passenger.living().is_some() {
                passenger.reset_fall_distance();
            }
        }
        if let Some(player) = passenger.as_any_mut().downcast_mut::<ServerPlayer>() {
            let moved = player.base().position().subtract(before);
            player.check_riding_statistics(self, moved.x, moved.y, moved.z);
        }
        passenger.after_ride_tick(self);
    }

    /// `CriteriaTriggers.START_RIDING_TRIGGER` for every player among the indirect passengers of the vehicle (pre-order);
    /// `rider` is at hand, the other passengers are in storage.
    fn trigger_start_riding(&mut self, rider: &mut dyn Entity, vehicle_passengers: &[EntityId]) {
        fn collect(
            level: &ServerLevel,
            rider: &dyn Entity,
            ids: &[EntityId],
            players: &mut Vec<EntityId>,
        ) {
            for &id in ids {
                let Some(entity) = (if id == rider.id() {
                    Some(rider)
                } else {
                    level.entity_manager.get_any(id)
                }) else {
                    continue;
                };
                if entity.is_player() {
                    players.push(id);
                }
                collect(level, rider, &entity.base().passengers, players);
            }
        }
        let mut players = Vec::new();
        collect(self, &*rider, vehicle_passengers, &mut players);
        let rider_id = rider.id();
        let mut others = Vec::new();
        for id in players {
            if id == rider_id {
                if let Some(player) = rider.as_any_mut().downcast_mut::<ServerPlayer>() {
                    CriteriaTriggers::START_RIDING_TRIGGER.trigger(self, player);
                }
            } else {
                others.push(id);
            }
        }
        if others.is_empty() {
            return;
        }
        let trigger_others = move |level: &mut ServerLevel| {
            for id in others {
                level.with_player(id, |level, player| {
                    CriteriaTriggers::START_RIDING_TRIGGER.trigger(level, player);
                });
            }
        };
        // the other players' conditions look the rider up in the level: wait until it is back in storage
        if self.entity_manager.has_entity_with_id(rider_id) {
            self.after_put_back_ops
                .entry(rider_id)
                .or_default()
                .push(Box::new(trigger_others));
        } else {
            trigger_others(self);
        }
    }
}
