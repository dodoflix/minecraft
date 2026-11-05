//! `net.minecraft.advancements.predicates.entity.PlayerPredicate`.

use super::entity_predicate::{EntityPredicate, EntitySubPredicateImpl};
use crate::advancements::predicates::food_predicate::FoodPredicate;
use crate::advancements::predicates::game_type_predicate::GameTypePredicate;
use crate::advancements::predicates::input_predicate::InputPredicate;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, ServerPlayer};
use crate::world::item::calculate_view_vector;
use crate::world::level::collision::{BlockGetterExt, ClipBlock, ClipContext, ClipFluid};
use mc_core::ident::Identifier;
use mc_core::phys::{Vec3, AABB};
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, Either, BOOL, STRING};
use mc_nbt::Tag;
use mc_protocol::components::holder::registry_name_codec;
use mc_protocol::components::Ints;
use std::sync::LazyLock;

/// `PlayerPredicate.LOOKING_AT_RANGE`.
pub const LOOKING_AT_RANGE: i32 = 100;

/// `PlayerPredicate.AdvancementPredicate`: `AdvancementDonePredicate` or `AdvancementCriterionsPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub enum AdvancementPredicate {
    Done(bool),
    /// `Object2BooleanMap<String>`: criterion -> must be done.
    Criterions(Vec<(String, bool)>),
}

impl AdvancementPredicate {
    fn codec() -> Codec<AdvancementPredicate> {
        BOOL.either(&STRING.unbounded_map(&BOOL)).xmap(
            |e| match e {
                Either::Left(done) => AdvancementPredicate::Done(done),
                Either::Right(criterions) => AdvancementPredicate::Criterions(criterions),
            },
            |p: &AdvancementPredicate| match p {
                AdvancementPredicate::Done(done) => Either::Left(*done),
                AdvancementPredicate::Criterions(criterions) => Either::Right(criterions.clone()),
            },
        )
    }

    /// `test(AdvancementProgress)`.
    pub fn test(&self, progress: &super::support::AdvancementProgressSnapshot) -> bool {
        match self {
            AdvancementPredicate::Done(state) => progress.done == *state,
            AdvancementPredicate::Criterions(criterions) => {
                criterions.iter().all(|(name, want)| progress.criteria.iter().find(|(n, _)| n == name).is_some_and(|(_, done)| done == want))
            }
        }
    }
}

/// `PlayerPredicate.StatMatcher<T>`: the stat of `stat_type` for the registry entry `stat` must be within `range`.
#[derive(Clone, Debug, PartialEq)]
pub struct StatMatcher {
    /// `BuiltInRegistries.STAT_TYPE` id (`minecraft:mined`, `minecraft:custom`, ...).
    pub stat_type: Identifier,
    /// The entry of the stat type's registry (`StatType.getRegistry()`).
    pub stat: Identifier,
    pub range: Ints,
}

/// `Stats.*` stat types and the registry of their values.
fn stat_registry(stat_type: &str) -> Option<&'static str> {
    Some(match stat_type {
        "mined" => "minecraft:block",
        "crafted" | "used" | "broken" | "picked_up" | "dropped" => "minecraft:item",
        "killed" | "killed_by" => "minecraft:entity_type",
        "custom" => "minecraft:custom_stat",
        _ => return None,
    })
}

#[derive(Clone)]
struct StatFields {
    stat: Identifier,
    range: Ints,
}

impl StatMatcher {
    /// `StatMatcher.CODEC`: `STAT_TYPE.byNameCodec().dispatch(type, ...)`.
    pub fn codec() -> Codec<StatMatcher> {
        static C: LazyLock<Codec<StatMatcher>> = LazyLock::new(|| {
            let type_codec = IDENTIFIER.flat_xmap(
                |name: Identifier| {
                    if name.namespace() == "minecraft" && stat_registry(name.path()).is_some() {
                        mc_dfu::DataResult::success(name)
                    } else {
                        let m = format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:stat_type]: {name}");
                        mc_dfu::DataResult::error(move || m.clone())
                    }
                },
                mc_dfu::DataResult::success_ref,
            );
            type_codec.dispatch(
                "type",
                |m: &StatMatcher| m.stat_type.clone(),
                |stat_type: &Identifier| {
                    let registry = stat_registry(stat_type.path()).expect("validated by the type codec");
                    let stat_type = stat_type.clone();
                    mc_dfu::record!(StatFields {
                        stat: registry_name_codec(registry).field_of("stat"),
                        range: Ints::codec().optional_field_of_default("value", Ints::any()),
                    })
                    .xmap(
                        move |f| StatMatcher { stat_type: stat_type.clone(), stat: f.stat, range: f.range },
                        |m: &StatMatcher| StatFields { stat: m.stat.clone(), range: m.range },
                    )
                },
            )
        });
        C.clone()
    }

    /// `matches(StatsCounter)`.
    pub fn matches(&self, player: &dyn Entity) -> bool {
        self.range.matches(player.get_stat_value(&self.stat_type, &self.stat))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerPredicate {
    pub level: Ints,
    pub food: FoodPredicate,
    pub game_type: GameTypePredicate,
    pub stats: Vec<StatMatcher>,
    /// `Object2BooleanMap<ResourceKey<Recipe<?>>>`: recipe -> must be known.
    pub recipes: Vec<(Identifier, bool)>,
    pub advancements: Vec<(Identifier, AdvancementPredicate)>,
    pub looking_at: Option<EntityPredicate>,
    pub input: Option<InputPredicate>,
}

/// `ServerRecipeBook.contains(recipe)` over the saved `ServerRecipeBook.Packed` tag.
fn recipe_book_contains(player: &ServerPlayer, recipe: &Identifier) -> bool {
    player.recipe_book.contains(recipe)
}

/// `ProjectileUtil.getEntityHitResult(level, source, from, to, box, matching, margin)`: the nearest entity whose box
/// (inflated by `margin`) the segment crosses.
fn get_entity_hit_result(level: &ServerLevel, source: &dyn Entity, from: Vec3, to: Vec3, area: &AABB, margin: f64) -> Option<crate::world::entity::EntityId> {
    let mut nearest = f64::MAX;
    let mut hit = None;
    for id in level.get_entities(Some(source.id()), area, |e| !e.is_spectator()) {
        let Some(entity) = level.get_entity(id) else { continue };
        let bb = entity.get_bounding_box().inflate_scalar(margin);
        if let Some(location) = bb.clip(from, to) {
            let dd = from.distance_to_sqr(location);
            if dd < nearest {
                hit = Some(id);
                nearest = dd;
            }
        }
    }
    hit
}

/// `LivingEntity.hasLineOfSight(target)`.
fn has_line_of_sight(level: &ServerLevel, viewer: &dyn Entity, target: &dyn Entity) -> bool {
    let from = Vec3::new(viewer.base().x(), viewer.base().get_eye_y(), viewer.base().z());
    let to = Vec3::new(target.base().x(), target.base().get_eye_y(), target.base().z());
    if to.distance_to(from) > 128.0 {
        return false;
    }
    let context = ClipContext::new(from, to, ClipBlock::Collider, ClipFluid::None, crate::world::entity::movement::collision_context(viewer));
    level.clip(&context).miss
}

impl EntitySubPredicateImpl for PlayerPredicate {
    fn codec() -> Codec<Self> {
        static C: LazyLock<Codec<PlayerPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(PlayerPredicate {
                level: Ints::codec().optional_field_of_default("level", Ints::any()),
                food: FoodPredicate::codec().optional_field_of_default("food", FoodPredicate::any()),
                game_type: GameTypePredicate::codec().optional_field_of_default("gamemode", GameTypePredicate::any()),
                stats: StatMatcher::codec().list_of().optional_field_of_default("stats", Vec::new()),
                recipes: IDENTIFIER.unbounded_map(&BOOL).optional_field_of_default("recipes", Vec::new()),
                advancements: IDENTIFIER.unbounded_map(&AdvancementPredicate::codec()).optional_field_of_default("advancements", Vec::new()),
                looking_at: EntityPredicate::lazy_codec().optional_field_of("looking_at"),
                input: InputPredicate::codec().optional_field_of("input"),
            })
            .codec()
        });
        C.clone()
    }

    fn matches<'a>(&self, entity: &'a dyn Entity, level: &'a ServerLevel, _position: Option<Vec3>, lent: &[&'a dyn Entity]) -> bool {
        let Some(player) = entity.as_any().downcast_ref::<ServerPlayer>() else { return false };
        if !self.level.matches(player.player.experience_level)
            || !self.food.matches(&player.player.food_data)
            || !self.game_type.matches(player.game_mode.game_mode_for_player)
            || !self.stats.iter().all(|s| s.matches(entity))
            || !self.recipes.iter().all(|(recipe, want)| recipe_book_contains(player, recipe) == *want)
        {
            return false;
        }
        for (id, predicate) in &self.advancements {
            if !entity.advancement_progress(id).is_some_and(|progress| predicate.test(&progress)) {
                return false;
            }
        }
        if let Some(looking_at) = &self.looking_at {
            let from = player.player.living.entity.get_eye_position();
            let view = calculate_view_vector(player.player.living.entity.x_rot(), player.get_y_head_rot());
            let range = f64::from(LOOKING_AT_RANGE);
            let to = Vec3::new(from.x + view.x * range, from.y + view.y * range, from.z + view.z * range);
            let area = AABB::from_corners(from, to).inflate_scalar(1.0);
            let Some(target) = get_entity_hit_result(level, entity, from, to, &area, 0.0).and_then(|id| level.get_entity(id)) else { return false };
            if !looking_at.matches_lent(level, Some(entity.position()), target, lent) || !has_line_of_sight(level, entity, target) {
                return false;
            }
        }
        self.input.as_ref().is_none_or(|input| input.matches(&player.last_client_input))
    }
}
