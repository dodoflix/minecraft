//! `net.minecraft.advancements.predicates.DamagePredicate` (the damage dealt to a player, for `player_hurt_entity`
//! style triggers; not the item durability `DamagePredicate` of `core.component.predicates`).

use super::damage_source_predicate::DamageSourcePredicate;
use super::entity::EntityPredicate;
use crate::server::level::ServerLevel;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;
use mc_core::ident::Identifier;
use mc_dfu::{Codec, BOOL};
use mc_protocol::components::Doubles;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct DamagePredicate {
    pub dealt_damage: Doubles,
    pub taken_damage: Doubles,
    pub source_entity: Option<EntityPredicate>,
    pub blocked: Option<bool>,
    pub type_: Option<DamageSourcePredicate>,
}

impl DamagePredicate {
    pub fn codec() -> Codec<DamagePredicate> {
        static C: LazyLock<Codec<DamagePredicate>> = LazyLock::new(|| {
            mc_dfu::record!(DamagePredicate {
                dealt_damage: Doubles::codec().optional_field_of_default("dealt", Doubles::any()),
                taken_damage: Doubles::codec().optional_field_of_default("taken", Doubles::any()),
                source_entity: EntityPredicate::lazy_codec().optional_field_of("source_entity"),
                blocked: BOOL.optional_field_of("blocked"),
                type_: DamageSourcePredicate::codec().optional_field_of("type"),
            })
            .codec()
        });
        C.clone()
    }

    /// `matches(player, source, originalDamage, actualDamage, blocked)` with the damage type, the direct and the
    /// causing entity of the source already resolved (the running entity is not in the level's storage).
    pub fn matches_parts(
        &self,
        level: &ServerLevel,
        player: &dyn Entity,
        damage_type: &Identifier,
        direct_entity: Option<&dyn Entity>,
        entity: Option<&dyn Entity>,
        is_direct: bool,
        original_damage: f32,
        actual_damage: f32,
        blocked: bool,
    ) -> bool {
        if !self.dealt_damage.matches(f64::from(original_damage)) || !self.taken_damage.matches(f64::from(actual_damage)) {
            return false;
        }
        if self.source_entity.as_ref().is_some_and(|p| !p.matches_from_player(level, player, entity)) {
            return false;
        }
        if self.blocked.is_some_and(|want| want != blocked) {
            return false;
        }
        self.type_.as_ref().is_none_or(|t| t.matches_parts(level, player.position(), damage_type, direct_entity, entity, is_direct))
    }

    /// `matches(player, source, originalDamage, actualDamage, blocked)`; the entities of the source are looked up in
    /// the level and distances are measured from `player`.
    pub fn matches(&self, level: &ServerLevel, player: &dyn Entity, source: &DamageSource, original_damage: f32, actual_damage: f32, blocked: bool) -> bool {
        let direct = source.get_direct_entity().and_then(|id| level.get_entity(id));
        let causing = source.get_entity().and_then(|id| level.get_entity(id));
        self.matches_parts(level, player, &source.damage_type, direct, causing, source.is_direct(), original_damage, actual_damage, blocked)
    }
}
