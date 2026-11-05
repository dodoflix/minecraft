//! `EntityFlagsPredicate`.

use super::entity_predicate::EntitySubPredicateImpl;
use super::support;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;
use mc_dfu::{Codec, BOOL};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct EntityFlagsPredicate {
    pub is_on_ground: Option<bool>,
    pub is_on_fire: Option<bool>,
    pub is_crouching: Option<bool>,
    pub is_sprinting: Option<bool>,
    pub is_swimming: Option<bool>,
    pub is_flying: Option<bool>,
    pub is_baby: Option<bool>,
    pub is_in_water: Option<bool>,
    pub is_fall_flying: Option<bool>,
}

impl EntityFlagsPredicate {
    /// `matches(Entity)`.
    pub fn matches_entity(&self, entity: &dyn Entity) -> bool {
        let differs = |want: Option<bool>, actual: bool| want.is_some_and(|w| w != actual);
        if differs(self.is_on_ground, entity.base().on_ground())
            || differs(self.is_on_fire, entity.is_on_fire())
            || differs(self.is_crouching, support::is_crouching(entity))
            || differs(self.is_sprinting, support::is_sprinting(entity))
            || differs(self.is_swimming, support::is_swimming(entity))
        {
            return false;
        }
        if let Some(want) = self.is_flying {
            let flying = entity.living().is_some() && (support::is_fall_flying(entity) || entity.player().is_some_and(|p| p.abilities.flying));
            if flying != want {
                return false;
            }
        }
        if differs(self.is_in_water, entity.base().was_touching_water) {
            return false;
        }
        if entity.living().is_some() && differs(self.is_fall_flying, support::is_fall_flying(entity)) {
            return false;
        }
        match self.is_baby {
            Some(want) if entity.living().is_some() => entity.is_baby() == want,
            _ => true,
        }
    }
}

impl EntitySubPredicateImpl for EntityFlagsPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<EntityFlagsPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(EntityFlagsPredicate {
                is_on_ground: BOOL.optional_field_of("is_on_ground"),
                is_on_fire: BOOL.optional_field_of("is_on_fire"),
                is_crouching: BOOL.optional_field_of("is_sneaking"),
                is_sprinting: BOOL.optional_field_of("is_sprinting"),
                is_swimming: BOOL.optional_field_of("is_swimming"),
                is_flying: BOOL.optional_field_of("is_flying"),
                is_baby: BOOL.optional_field_of("is_baby"),
                is_in_water: BOOL.optional_field_of("is_in_water"),
                is_fall_flying: BOOL.optional_field_of("is_fall_flying"),
            })
            .codec()
        });
        C.clone()
    }

    fn matches(&self, entity: &dyn Entity, _level: &ServerLevel, _position: Option<Vec3>) -> bool {
        self.matches_entity(entity)
    }
}
