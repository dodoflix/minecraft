//! The `type_specific/*` sub predicates. Their subject entity classes are not all modelled yet: each one asks the
//! entity through the matching `Entity` seam hook (`sheep_sheared`, `cube_mob_size`, `raider_state`,
//! `fishing_hook_open_water`, `lightning_bolt_state`) which is `None` for every other class.

use super::entity_predicate::{EntityPredicate, EntitySubPredicateImpl};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;
use mc_dfu::{Codec, BOOL};
use mc_protocol::components::Ints;
use std::sync::LazyLock;

/// `LightningBoltPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct LightningBoltPredicate {
    pub blocks_set_on_fire: Ints,
    pub entity_struck: Option<EntityPredicate>,
}

impl EntitySubPredicateImpl for LightningBoltPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<LightningBoltPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(LightningBoltPredicate {
                blocks_set_on_fire: Ints::codec().optional_field_of_default("blocks_set_on_fire", Ints::any()),
                entity_struck: EntityPredicate::lazy_codec().optional_field_of("entity_struck"),
            })
            .codec()
        });
        C.clone()
    }

    fn matches<'a>(&self, entity: &'a dyn Entity, level: &'a ServerLevel, position: Option<Vec3>, lent: &[&'a dyn Entity]) -> bool {
        let Some(bolt) = entity.lightning_bolt_state() else { return false };
        self.blocks_set_on_fire.matches(bolt.blocks_set_on_fire)
            && self
                .entity_struck
                .as_ref()
                .is_none_or(|struck| bolt.hit_entities.iter().any(|&id| struck.matches_nullable_lent(level, position, super::support::resolve(level, lent, id), lent)))
    }
}

/// `FishingHookPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct FishingHookPredicate {
    pub in_open_water: Option<bool>,
}

impl EntitySubPredicateImpl for FishingHookPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<FishingHookPredicate>> =
            LazyLock::new(|| mc_dfu::record!(FishingHookPredicate { in_open_water: BOOL.optional_field_of("in_open_water") }).codec());
        C.clone()
    }

    fn matches<'a>(&self, entity: &'a dyn Entity, _level: &'a ServerLevel, _position: Option<Vec3>, _lent: &[&'a dyn Entity]) -> bool {
        match self.in_open_water {
            None => true,
            Some(want) => entity.fishing_hook_open_water() == Some(want),
        }
    }
}

/// `CubeMobPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct CubeMobPredicate {
    pub size: Ints,
}

impl EntitySubPredicateImpl for CubeMobPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<CubeMobPredicate>> =
            LazyLock::new(|| mc_dfu::record!(CubeMobPredicate { size: Ints::codec().optional_field_of_default("size", Ints::any()) }).codec());
        C.clone()
    }

    fn matches<'a>(&self, entity: &'a dyn Entity, _level: &'a ServerLevel, _position: Option<Vec3>, _lent: &[&'a dyn Entity]) -> bool {
        entity.cube_mob_size().is_some_and(|size| self.size.matches(size))
    }
}

/// `RaiderPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct RaiderPredicate {
    pub has_raid: bool,
    pub is_captain: bool,
}

impl EntitySubPredicateImpl for RaiderPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<RaiderPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(RaiderPredicate {
                has_raid: BOOL.optional_field_of_default("has_raid", false),
                is_captain: BOOL.optional_field_of_default("is_captain", false),
            })
            .codec()
        });
        C.clone()
    }

    fn matches<'a>(&self, entity: &'a dyn Entity, level: &'a ServerLevel, _position: Option<Vec3>, _lent: &[&'a dyn Entity]) -> bool {
        crate::world::entity::raid::raider::raider_state(entity, level) == Some((self.has_raid, self.is_captain))
    }
}

/// `SheepPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct SheepPredicate {
    pub sheared: Option<bool>,
}

impl EntitySubPredicateImpl for SheepPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<SheepPredicate>> =
            LazyLock::new(|| mc_dfu::record!(SheepPredicate { sheared: BOOL.optional_field_of("sheared") }).codec());
        C.clone()
    }

    fn matches<'a>(&self, entity: &'a dyn Entity, _level: &'a ServerLevel, _position: Option<Vec3>, _lent: &[&'a dyn Entity]) -> bool {
        entity.sheep_sheared().is_some_and(|sheared| self.sheared.is_none_or(|want| sheared == want))
    }
}
