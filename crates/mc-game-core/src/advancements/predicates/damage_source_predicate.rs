//! `net.minecraft.advancements.predicates.DamageSourcePredicate`.

use super::entity::EntityPredicate;
use super::tag_predicate::TagPredicate;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;
use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_dfu::{BOOL, Codec};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct DamageSourcePredicate {
    pub tags: Vec<TagPredicate>,
    pub direct_entity: Option<EntityPredicate>,
    pub source_entity: Option<EntityPredicate>,
    pub is_direct: Option<bool>,
}

impl DamageSourcePredicate {
    pub fn codec() -> Codec<DamageSourcePredicate> {
        static C: LazyLock<Codec<DamageSourcePredicate>> = LazyLock::new(|| {
            mc_dfu::record!(DamageSourcePredicate {
                tags: TagPredicate::codec("damage_type")
                    .list_of()
                    .optional_field_of_default("tags", Vec::new()),
                direct_entity: EntityPredicate::lazy_codec().optional_field_of("direct_entity"),
                source_entity: EntityPredicate::lazy_codec().optional_field_of("source_entity"),
                is_direct: BOOL.optional_field_of("is_direct"),
            })
            .codec()
        });
        C.clone()
    }

    /// `matches(level, position, source)` with the damage type, the direct and the causing entity already resolved
    /// (the running entity is not in the level's storage, so the caller that knows it passes it).
    pub fn matches_parts<'a>(
        &self,
        level: &'a ServerLevel,
        position: Vec3,
        damage_type: &Identifier,
        direct_entity: Option<&'a dyn Entity>,
        entity: Option<&'a dyn Entity>,
        is_direct: bool,
    ) -> bool {
        let lent: Vec<&'a dyn Entity> = direct_entity.into_iter().chain(entity).collect();
        if !self.tags.iter().all(|tag| tag.matches(level, damage_type)) {
            return false;
        }
        if self
            .direct_entity
            .as_ref()
            .is_some_and(|p| !p.matches_nullable_lent(level, Some(position), direct_entity, &lent))
        {
            return false;
        }
        if self
            .source_entity
            .as_ref()
            .is_some_and(|p| !p.matches_nullable_lent(level, Some(position), entity, &lent))
        {
            return false;
        }
        self.is_direct.is_none_or(|want| want == is_direct)
    }

    /// `matches(level, position, source)`; the entities of the source are looked up in the level.
    pub fn matches<'a>(
        &self,
        level: &'a ServerLevel,
        position: Vec3,
        source: &DamageSource,
    ) -> bool {
        let direct = source
            .get_direct_entity()
            .and_then(|id| level.get_entity(id));
        let causing = source.get_entity().and_then(|id| level.get_entity(id));
        self.matches_parts(
            level,
            position,
            &source.damage_type,
            direct,
            causing,
            source.is_direct(),
        )
    }
}
