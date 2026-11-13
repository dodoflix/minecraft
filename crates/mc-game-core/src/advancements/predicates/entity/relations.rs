//! `VehiclePredicate`, `PassengerPredicate`, `TargetedEntityPredicate`: sub predicates that recurse into another entity.

use super::entity_predicate::{EntityPredicate, EntitySubPredicateImpl};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;
use mc_dfu::Codec;

/// `VehiclePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct VehiclePredicate {
    pub vehicle: EntityPredicate,
}

impl EntitySubPredicateImpl for VehiclePredicate {
    fn codec() -> Codec<Self> {
        EntityPredicate::lazy_codec().xmap(
            |vehicle| VehiclePredicate { vehicle },
            |p: &VehiclePredicate| p.vehicle.clone(),
        )
    }

    fn matches<'a>(
        &self,
        entity: &'a dyn Entity,
        level: &'a ServerLevel,
        position: Option<Vec3>,
    ) -> bool {
        self.vehicle.matches_nullable(
            level,
            position,
            entity.base().vehicle.and_then(|id| level.get_entity(id)),
        )
    }
}

/// `PassengerPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct PassengerPredicate {
    pub passenger: EntityPredicate,
}

impl EntitySubPredicateImpl for PassengerPredicate {
    fn codec() -> Codec<Self> {
        EntityPredicate::lazy_codec().xmap(
            |passenger| PassengerPredicate { passenger },
            |p: &PassengerPredicate| p.passenger.clone(),
        )
    }

    fn matches<'a>(
        &self,
        entity: &'a dyn Entity,
        level: &'a ServerLevel,
        position: Option<Vec3>,
    ) -> bool {
        entity.base().passengers.iter().any(|&id| {
            self.passenger
                .matches_nullable(level, position, level.get_entity(id))
        })
    }
}

/// `TargetedEntityPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetedEntityPredicate {
    pub targeted_entity: EntityPredicate,
}

impl EntitySubPredicateImpl for TargetedEntityPredicate {
    fn codec() -> Codec<Self> {
        EntityPredicate::lazy_codec().xmap(
            |targeted_entity| TargetedEntityPredicate { targeted_entity },
            |p: &TargetedEntityPredicate| p.targeted_entity.clone(),
        )
    }

    fn matches<'a>(
        &self,
        entity: &'a dyn Entity,
        level: &'a ServerLevel,
        position: Option<Vec3>,
    ) -> bool {
        self.targeted_entity.matches_nullable(
            level,
            position,
            entity.get_target().and_then(|id| level.get_entity(id)),
        )
    }
}
