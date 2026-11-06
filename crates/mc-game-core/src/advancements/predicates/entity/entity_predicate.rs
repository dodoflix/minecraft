//! `net.minecraft.advancements.predicates.entity.EntityPredicate`, `EntitySubPredicate` and `EntitySubPredicates`.
//!
//! An `EntityPredicate` is a map from sub predicate type (registry `entity_sub_predicate_type`) to sub predicate;
//! its JSON is `{"minecraft:distance": {...}, "minecraft:flags": {...}, ...}` (the namespace may be omitted).

use super::basic::*;
use super::equipment::EntityEquipmentPredicate;
use super::flags::EntityFlagsPredicate;
use super::movement::MovementPredicate;
use super::player_predicate::PlayerPredicate;
use super::relations::*;
use super::type_specific::*;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_dfu::extra::{IDENTIFIER, dispatched_map};
use mc_dfu::{Codec, DataResult};
use std::sync::LazyLock;

/// A sub predicate class: its codec and `EntitySubPredicate.matches(entity, level, position)`.
pub trait EntitySubPredicateImpl: Sized {
    fn codec() -> Codec<Self>;
    fn matches<'a>(
        &self,
        entity: &'a dyn Entity,
        level: &'a ServerLevel,
        position: Option<Vec3>,
        lent: &[&'a dyn Entity],
    ) -> bool;
}

macro_rules! entity_sub_predicates {
    ($($variant:ident($name:literal, $ty:ty)),* $(,)?) => {
        /// `EntitySubPredicate` implementations in `EntitySubPredicates.bootstrap` order.
        #[derive(Clone, Debug, PartialEq)]
        pub enum EntitySubPredicate {
            $($variant($ty)),*
        }

        impl EntitySubPredicate {
            /// The registry name (`entity_sub_predicate_type`) of this predicate's type.
            pub fn type_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name),*
                }
            }

            /// `matches(entity, level, position)`.
            pub fn matches<'a>(&self, entity: &'a dyn Entity, level: &'a ServerLevel, position: Option<Vec3>, lent: &[&'a dyn Entity]) -> bool {
                match self {
                    $(Self::$variant(p) => EntitySubPredicateImpl::matches(p, entity, level, position, lent)),*
                }
            }
        }

        /// The value codec of every sub predicate type, by registry name.
        static VALUE_CODECS: LazyLock<Vec<(&'static str, Codec<EntitySubPredicate>)>> = LazyLock::new(|| {
            vec![$(($name, <$ty as EntitySubPredicateImpl>::codec().xmap(
                EntitySubPredicate::$variant,
                |p: &EntitySubPredicate| match p {
                    EntitySubPredicate::$variant(x) => x.clone(),
                    _ => unreachable!("dispatched by type name"),
                },
            ))),*]
        });
    };
}

entity_sub_predicates! {
    EntityType("entity_type", EntityTypePredicate),
    Location("location", EntityLocationPredicate),
    SteppingOn("stepping_on", SteppingOnPredicate),
    MovementAffectedBy("movement_affected_by", MovementAffectedByPredicate),
    Distance("distance", DistanceToPlayerPredicate),
    Movement("movement", MovementPredicate),
    Effects("effects", EntityEffectsPredicate),
    Nbt("nbt", EntityNbtPredicate),
    Flags("flags", EntityFlagsPredicate),
    Equipment("equipment", EntityEquipmentPredicate),
    PeriodicTick("periodic_tick", PeriodicEntityTickPredicate),
    Vehicle("vehicle", VehiclePredicate),
    Passenger("passenger", PassengerPredicate),
    TargetedEntity("targeted_entity", TargetedEntityPredicate),
    Team("team", TeamPredicate),
    Slots("slots", EntitySlotsPredicate),
    Components("components", EntityExactDataComponentsPredicate),
    Predicates("predicates", EntityPartialComponentsPredicate),
    EntityTags("entity_tags", EntityTagPredicate),
    Lightning("type_specific/lightning", LightningBoltPredicate),
    FishingHook("type_specific/fishing_hook", FishingHookPredicate),
    Player("type_specific/player", PlayerPredicate),
    CubeMob("type_specific/cube_mob", CubeMobPredicate),
    Raider("type_specific/raider", RaiderPredicate),
    Sheep("type_specific/sheep", SheepPredicate),
}

impl EntitySubPredicate {
    /// `PREDICATE_TYPE_ORDER`: the entity type first, then the cheap tests, then the ones that recurse into other
    /// entities, the NBT comparison last.
    fn sort_rank(&self) -> i32 {
        match self {
            Self::EntityType(_) => -1,
            Self::Vehicle(_)
            | Self::Passenger(_)
            | Self::TargetedEntity(_)
            | Self::Lightning(_)
            | Self::Player(_) => 1,
            Self::Nbt(_) => 2,
            _ => 0,
        }
    }
}

/// `BuiltInRegistries.ENTITY_SUB_PREDICATE_TYPE.byNameCodec()` as the index into [`VALUE_CODECS`].
fn type_key_codec() -> Codec<usize> {
    IDENTIFIER.flat_xmap(
        |name: Identifier| match VALUE_CODECS.iter().position(|(n, _)| name.namespace() == "minecraft" && name.path() == *n) {
            Some(i) => DataResult::success(i),
            None => {
                let m = format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:entity_sub_predicate_type]: {name}");
                DataResult::error(move || m.clone())
            }
        },
        |i: &usize| DataResult::success(Identifier::with_default_namespace(VALUE_CODECS[*i].0).expect("valid path")),
    )
}

/// `EntityPredicate`.
#[derive(Clone, Debug)]
pub struct EntityPredicate {
    parts: Vec<EntitySubPredicate>,
    /// Some part looks at other entities (vehicle, passenger, target, struck entities, looked-at entity).
    recurses: bool,
    /// Indices into `parts` in `combine` order (stable sort by [`EntitySubPredicate::sort_rank`]).
    order: Vec<usize>,
}

impl PartialEq for EntityPredicate {
    fn eq(&self, other: &Self) -> bool {
        self.parts.len() == other.parts.len() && self.parts.iter().all(|p| other.parts.contains(p))
    }
}

impl EntityPredicate {
    /// `new EntityPredicate(parts)`; at most one part per type.
    pub fn new(parts: Vec<EntitySubPredicate>) -> EntityPredicate {
        let mut order: Vec<usize> = (0..parts.len()).collect();
        order.sort_by_key(|&i| parts[i].sort_rank());
        let recurses = parts.iter().any(|p| {
            matches!(
                p,
                EntitySubPredicate::Vehicle(_)
                    | EntitySubPredicate::Passenger(_)
                    | EntitySubPredicate::TargetedEntity(_)
                    | EntitySubPredicate::Lightning(_)
                    | EntitySubPredicate::Player(_)
            )
        });
        EntityPredicate {
            parts,
            recurses,
            order,
        }
    }

    /// `EntityPredicate.CODEC`.
    pub fn codec() -> Codec<EntityPredicate> {
        static C: LazyLock<Codec<EntityPredicate>> = LazyLock::new(|| {
            dispatched_map(&type_key_codec(), |i: &usize| {
                DataResult::success(VALUE_CODECS[*i].1.clone())
            })
            .xmap(
                |entries: Vec<(usize, EntitySubPredicate)>| {
                    EntityPredicate::new(entries.into_iter().map(|(_, p)| p).collect())
                },
                |p: &EntityPredicate| {
                    p.parts
                        .iter()
                        .map(|part| {
                            (
                                VALUE_CODECS
                                    .iter()
                                    .position(|(n, _)| *n == part.type_name())
                                    .expect("registered"),
                                part.clone(),
                            )
                        })
                        .collect()
                },
            )
        });
        C.clone()
    }

    /// The codec for use inside sub predicates (`EntityPredicate.CODEC` is referenced before its static is built).
    pub(crate) fn lazy_codec() -> Codec<EntityPredicate> {
        Codec::lazy(EntityPredicate::codec)
    }

    pub fn parts(&self) -> &[EntitySubPredicate] {
        &self.parts
    }

    /// `matches(level, position, entity)` for a present entity; `position` is the origin of the distance checks
    /// (`null` outside a player context).
    pub fn matches<'a>(
        &self,
        level: &'a ServerLevel,
        position: Option<Vec3>,
        entity: &'a dyn Entity,
    ) -> bool {
        self.matches_lent(level, position, entity, &[])
    }

    /// `matches` with entities that are not in the level's storage because they are being ticked (`lent`); `entity`
    /// itself is lent to the nested predicates automatically, so a vehicle/passenger/target lookup that leads back to
    /// it finds it.
    pub fn matches_lent<'a>(
        &self,
        level: &'a ServerLevel,
        position: Option<Vec3>,
        entity: &'a dyn Entity,
        lent: &[&'a dyn Entity],
    ) -> bool {
        if self.recurses {
            let mut all: Vec<&'a dyn Entity> = lent.to_vec();
            all.push(entity);
            self.order
                .iter()
                .all(|&i| self.parts[i].matches(entity, level, position, &all))
        } else {
            self.order
                .iter()
                .all(|&i| self.parts[i].matches(entity, level, position, lent))
        }
    }

    /// `matches(level, position, @Nullable entity)`.
    pub fn matches_nullable<'a>(
        &self,
        level: &'a ServerLevel,
        position: Option<Vec3>,
        entity: Option<&'a dyn Entity>,
    ) -> bool {
        self.matches_nullable_lent(level, position, entity, &[])
    }

    pub fn matches_nullable_lent<'a>(
        &self,
        level: &'a ServerLevel,
        position: Option<Vec3>,
        entity: Option<&'a dyn Entity>,
        lent: &[&'a dyn Entity],
    ) -> bool {
        entity.is_some_and(|e| self.matches_lent(level, position, e, lent))
    }

    /// `matches(ServerPlayer player, @Nullable Entity entity)`: distances are measured from the player.
    pub fn matches_from_player<'a>(
        &self,
        level: &'a ServerLevel,
        player: &'a dyn Entity,
        entity: Option<&'a dyn Entity>,
    ) -> bool {
        self.matches_nullable_lent(level, Some(player.position()), entity, &[player])
    }
}
