//! The `requirements` of conditional effects: a `Holder<LootItemCondition>` (`LootItemCondition.CODEC`) evaluated in the
//! `LootContext` of the effect's parameter set (`Enchantment.damageContext/itemContext/locationContext/entityContext/blockHitContext`).

use mc_dfu::Codec;

use super::actors::Actors;
use super::evaluate::ContextKind;
use crate::server::level::ServerLevel;
use crate::world::level::storage::loot::context::{LootContext, LootParams};
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::predicates::{ConditionHolder, condition_holder_codec};

/// `Optional<Holder<LootItemCondition>>` element.
pub type Requirement = ConditionHolder;

/// `LootItemCondition.CODEC`.
pub fn requirement_codec() -> Codec<Requirement> {
    condition_holder_codec()
}

/// `requirement.value().test(context)` with the context of `kind`.
pub fn requirement_test(level: &mut ServerLevel, actors: &Actors<'_>, requirement: &Requirement, enchantment_level: i32, kind: &ContextKind<'_>) -> bool {
    let Some(condition) = requirement.try_value(&level.registries) else { return false };
    // the context draws from the level's own random (a shared handle: the draws advance the level's sequence)
    let shared = level.tick_state.random.clone();
    let result = {
        let lvl: &ServerLevel = level;
        let entity = |id| actors.get(lvl, id);
        let params = match kind {
            ContextKind::Damage { victim, source } => {
                let victim_e = entity(*victim);
                match victim_e {
                    None => None,
                    Some(v) => Some(
                        LootParams::builder(lvl)
                            .this_entity(v)
                            .enchantment_level(enchantment_level)
                            .origin(v.position())
                            .damage_source(source)
                            .attacking_entity(source.get_entity().and_then(entity))
                            .direct_attacking_entity(source.get_direct_entity().and_then(entity))
                            .create(&LootContextParamSets::ENCHANTED_DAMAGE),
                    ),
                }
            }
            ContextKind::Item { stack } => Some(LootParams::builder(lvl).tool(stack).enchantment_level(enchantment_level).create(&LootContextParamSets::ENCHANTED_ITEM)),
            ContextKind::Location { entity: id, active } => entity(*id).map(|e| {
                LootParams::builder(lvl)
                    .this_entity(e)
                    .enchantment_level(enchantment_level)
                    .origin(e.position())
                    .enchantment_active(*active)
                    .create(&LootContextParamSets::ENCHANTED_LOCATION)
            }),
            ContextKind::Entity { entity: id, position } => entity(*id).map(|e| {
                LootParams::builder(lvl).this_entity(e).enchantment_level(enchantment_level).origin(*position).create(&LootContextParamSets::ENCHANTED_ENTITY)
            }),
            ContextKind::HitBlock { entity: id, position, state } => entity(*id).map(|e| {
                LootParams::builder(lvl)
                    .this_entity(e)
                    .enchantment_level(enchantment_level)
                    .origin(*position)
                    .block_state(*state)
                    .create(&LootContextParamSets::HIT_BLOCK)
            }),
        };
        params.is_some_and(|params| {
            let mut context = LootContext::builder(&params).with_optional_random_source(Box::new(shared.clone())).create(None);
            condition.test(&mut context)
        })
    };
    result
}
