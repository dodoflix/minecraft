//! SEAM(EcPredicates): the advancement predicates loot conditions evaluate (`EntityPredicate`, `DamageSourcePredicate`,
//! `LocationPredicate`). Until `crate::advancements::predicates` lands these hold the parsed value untouched and never
//! match; the loot code only uses `codec()` / `matches`, so swapping the re-exports for the real types is the whole change.

use mc_core::phys::Vec3;
use mc_dfu::{Codec, Dynamic, PASSTHROUGH};

use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;

macro_rules! raw_predicate {
    ($name:ident) => {
        #[derive(Clone)]
        pub struct $name(Dynamic);
        impl $name {
            pub fn codec() -> Codec<$name> {
                PASSTHROUGH.xmap($name, |p: &$name| p.0.clone())
            }
        }
    };
}

raw_predicate!(EntityPredicate);
raw_predicate!(DamageSourcePredicate);
raw_predicate!(LocationPredicate);

impl EntityPredicate {
    pub fn matches(&self, _level: &ServerLevel, _origin: Option<Vec3>, _entity: &dyn Entity) -> bool {
        false
    }
}

impl DamageSourcePredicate {
    pub fn matches(&self, _level: &ServerLevel, _origin: Vec3, _source: &DamageSource) -> bool {
        false
    }
}

impl LocationPredicate {
    pub fn matches(&self, _level: &ServerLevel, _x: f64, _y: f64, _z: f64) -> bool {
        false
    }
}
