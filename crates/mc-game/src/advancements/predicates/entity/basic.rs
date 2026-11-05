//! The small `EntitySubPredicate` classes that wrap one other predicate or a handful of fields.

use super::entity_predicate::EntitySubPredicateImpl;
use super::support;
use crate::advancements::predicates::distance_predicate::DistancePredicate;
use crate::advancements::predicates::location_predicate::LocationPredicate;
use crate::advancements::predicates::mob_effects_predicate::MobEffectsPredicateEntityExt;
use crate::advancements::predicates::nbt_predicate::NbtPredicateEntityExt;
use crate::advancements::predicates::slots_predicate::SlotsPredicate;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;
use mc_dfu::extra::POSITIVE_INT;
use mc_dfu::{Codec, STRING};
use mc_protocol::components::{DataComponentExactPredicate, MobEffectsPredicate, NbtPredicate, PredicateMap, RegistrySet};
use std::sync::LazyLock;

/// `EntityTypePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityTypePredicate {
    pub types: RegistrySet<mc_data::EntityType>,
}

impl EntitySubPredicateImpl for EntityTypePredicate {
    fn codec() -> Codec<Self> {
        RegistrySet::<mc_data::EntityType>::codec().xmap(|types| EntityTypePredicate { types }, |p: &EntityTypePredicate| p.types.clone())
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.types.contains(entity.get_type())
    }
}

/// `EntityLocationPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityLocationPredicate {
    pub predicate: LocationPredicate,
}

impl EntitySubPredicateImpl for EntityLocationPredicate {
    fn codec() -> Codec<Self> {
        LocationPredicate::codec().xmap(|predicate| EntityLocationPredicate { predicate }, |p: &EntityLocationPredicate| p.predicate.clone())
    }

    fn matches(&self, entity: &dyn Entity, level: &ServerLevel, _position: Option<Vec3>) -> bool {
        let b = entity.base();
        self.predicate.matches(level, b.x(), b.y(), b.z())
    }
}

/// `Vec3.atCenterOf(pos)`.
fn center_of(pos: mc_core::pos::BlockPos) -> Vec3 {
    Vec3::new(f64::from(pos.x()) + 0.5, f64::from(pos.y()) + 0.5, f64::from(pos.z()) + 0.5)
}

/// `SteppingOnPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct SteppingOnPredicate {
    pub predicate: LocationPredicate,
}

impl EntitySubPredicateImpl for SteppingOnPredicate {
    fn codec() -> Codec<Self> {
        LocationPredicate::codec().xmap(|predicate| SteppingOnPredicate { predicate }, |p: &SteppingOnPredicate| p.predicate.clone())
    }

    fn matches(&self, entity: &dyn Entity, level: &ServerLevel, _position: Option<Vec3>) -> bool {
        if !entity.base().on_ground() {
            return false;
        }
        let on_pos = center_of(support::get_on_pos(level, entity));
        self.predicate.matches(level, on_pos.x, on_pos.y, on_pos.z)
    }
}

/// `MovementAffectedByPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct MovementAffectedByPredicate {
    pub predicate: LocationPredicate,
}

impl EntitySubPredicateImpl for MovementAffectedByPredicate {
    fn codec() -> Codec<Self> {
        LocationPredicate::codec().xmap(|predicate| MovementAffectedByPredicate { predicate }, |p: &MovementAffectedByPredicate| p.predicate.clone())
    }

    fn matches(&self, entity: &dyn Entity, level: &ServerLevel, _position: Option<Vec3>) -> bool {
        let on_pos = center_of(support::get_block_pos_below_that_affects_my_movement(level, entity));
        self.predicate.matches(level, on_pos.x, on_pos.y, on_pos.z)
    }
}

/// `DistanceToPlayerPredicate`: distance from the origin passed to `EntityPredicate.matches`.
#[derive(Clone, Debug, PartialEq)]
pub struct DistanceToPlayerPredicate {
    pub distance: DistancePredicate,
}

impl EntitySubPredicateImpl for DistanceToPlayerPredicate {
    fn codec() -> Codec<Self> {
        DistancePredicate::codec().xmap(|distance| DistanceToPlayerPredicate { distance }, |p: &DistanceToPlayerPredicate| p.distance.clone())
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, position: Option<Vec3>) -> bool {
        let b = entity.base();
        position.is_some_and(|p| self.distance.matches(p.x, p.y, p.z, b.x(), b.y(), b.z()))
    }
}

/// `EntityEffectsPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityEffectsPredicate {
    pub effects: MobEffectsPredicate,
}

impl EntitySubPredicateImpl for EntityEffectsPredicate {
    fn codec() -> Codec<Self> {
        MobEffectsPredicate::codec().xmap(|effects| EntityEffectsPredicate { effects }, |p: &EntityEffectsPredicate| p.effects.clone())
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.effects.matches_entity(entity)
    }
}

/// `EntityNbtPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityNbtPredicate {
    pub nbt: NbtPredicate,
}

impl EntitySubPredicateImpl for EntityNbtPredicate {
    fn codec() -> Codec<Self> {
        NbtPredicate::codec().xmap(|nbt| EntityNbtPredicate { nbt }, |p: &EntityNbtPredicate| p.nbt.clone())
    }

    fn matches(&self, entity: &dyn Entity, level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.nbt.matches_entity(level, entity)
    }
}

/// `PeriodicEntityTickPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct PeriodicEntityTickPredicate {
    pub periodic_tick: i32,
}

impl EntitySubPredicateImpl for PeriodicEntityTickPredicate {
    fn codec() -> Codec<Self> {
        POSITIVE_INT.xmap(|periodic_tick| PeriodicEntityTickPredicate { periodic_tick }, |p: &PeriodicEntityTickPredicate| p.periodic_tick)
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        entity.base().tick_count % self.periodic_tick == 0
    }
}

/// `TeamPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct TeamPredicate {
    pub team: String,
}

impl EntitySubPredicateImpl for TeamPredicate {
    fn codec() -> Codec<Self> {
        STRING.xmap(|team| TeamPredicate { team }, |p: &TeamPredicate| p.team.clone())
    }

    fn matches(&self, entity: &dyn Entity, level: &ServerLevel, _position: Option<Vec3>) -> bool {
        support::get_team_name(level, entity).is_some_and(|name| name == self.team)
    }
}

/// `EntitySlotsPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntitySlotsPredicate {
    pub slots: SlotsPredicate,
}

impl EntitySubPredicateImpl for EntitySlotsPredicate {
    fn codec() -> Codec<Self> {
        SlotsPredicate::codec().xmap(|slots| EntitySlotsPredicate { slots }, |p: &EntitySlotsPredicate| p.slots.clone())
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.slots.matches(entity)
    }
}

/// `EntityExactDataComponentsPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityExactDataComponentsPredicate {
    pub predicate: DataComponentExactPredicate,
}

impl EntitySubPredicateImpl for EntityExactDataComponentsPredicate {
    fn codec() -> Codec<Self> {
        DataComponentExactPredicate::codec()
            .xmap(|predicate| EntityExactDataComponentsPredicate { predicate }, |p: &EntityExactDataComponentsPredicate| p.predicate.clone())
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.predicate.test(&entity.entity_components())
    }
}

/// `EntityPartialComponentsPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityPartialComponentsPredicate {
    pub predicates: PredicateMap,
}

impl EntitySubPredicateImpl for EntityPartialComponentsPredicate {
    fn codec() -> Codec<Self> {
        PredicateMap::codec().xmap(|predicates| EntityPartialComponentsPredicate { predicates }, |p: &EntityPartialComponentsPredicate| p.predicates.clone())
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        let components = entity.entity_components();
        self.predicates.0.iter().all(|p| p.matches(&components))
    }
}

/// `EntityTagPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityTagPredicate {
    pub any_of: Option<Vec<String>>,
    pub all_of: Option<Vec<String>>,
    pub none_of: Option<Vec<String>>,
}

impl EntityTagPredicate {
    /// `matches(Set<String> tags)`.
    pub fn matches_tags(&self, tags: &crate::world::entity::tag_set::JavaStringSet) -> bool {
        let contains_at_least_one = |wanted: &[String]| wanted.iter().any(|t| tags.contains(t));
        if self.any_of.as_ref().is_some_and(|a| !contains_at_least_one(a)) {
            return false;
        }
        if self.none_of.as_ref().is_some_and(|n| contains_at_least_one(n)) {
            return false;
        }
        self.all_of.as_ref().is_none_or(|a| a.iter().all(|t| tags.contains(t)))
    }
}

impl EntitySubPredicateImpl for EntityTagPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<EntityTagPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(EntityTagPredicate {
                any_of: STRING.list_of().optional_field_of("any_of"),
                all_of: STRING.list_of().optional_field_of("all_of"),
                none_of: STRING.list_of().optional_field_of("none_of"),
            })
            .codec()
        });
        C.clone()
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.matches_tags(&entity.base().tags)
    }
}
