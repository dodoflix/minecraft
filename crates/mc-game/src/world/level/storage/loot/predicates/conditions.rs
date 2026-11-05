//! Every `LootItemCondition` (`LootItemConditionTypes.bootstrap`).

use std::any::Any;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_dfu::extra::non_empty_list;
use mc_dfu::{BOOL, DataResult, FLOAT, INT, LONG, MapCodec, STRING, float_range, record};
use mc_protocol::components::values::predicates::{BlockPredicate, ItemPredicate};

use super::{ConditionHolder, ConditionHolderSet, ConditionRef, LootItemCondition, condition_holder_codec, condition_list_codec};
use crate::registry::keys;
use crate::world::level::storage::loot::context::{ContextKey, EntityTarget, LootContext};
use crate::world::level::storage::loot::enchant_api;
use crate::world::level::storage::loot::holder::{TypeTable, lift, registry_ref_codec};
use crate::world::level::storage::loot::predicate_api::{DamageSourcePredicate, EntityPredicate, LocationPredicate};
use crate::world::level::storage::loot::providers::number::{FloatHolder, IntHolder, float_holder_codec, int_holder_codec};
use crate::world::level::storage::loot::range::{FloatRangePredicate, IntRangePredicate};
use crate::world::level::storage::loot::server_access;
use crate::world::level::storage::loot::validation::{ValidationContext, validate_holder_field, validate_holder_set};
use crate::world::item::enchantment::LevelBasedValue;

macro_rules! condition_basics {
    ($name:literal) => {
        fn type_name(&self) -> &'static str {
            $name
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    };
}

fn up<C: LootItemCondition + 'static>(c: C) -> ConditionRef {
    Arc::new(c)
}

// ---- composites ---------------------------------------------------------------------------------------------------------

/// `InvertedLootItemCondition`.
#[derive(Clone)]
pub struct InvertedLootItemCondition {
    pub term: ConditionHolder,
}

impl LootItemCondition for InvertedLootItemCondition {
    condition_basics!("inverted");
    fn test(&self, context: &mut LootContext) -> bool {
        !self.term.value(context.get_resolver()).test(context)
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
        validate_holder_field(context, "term", &self.term);
    }
}

/// `AllOfCondition` / `AnyOfCondition`.
#[derive(Clone)]
pub struct CompositeLootItemCondition {
    pub all: bool,
    pub terms: ConditionHolderSet,
}

impl LootItemCondition for CompositeLootItemCondition {
    fn type_name(&self) -> &'static str {
        if self.all { "all_of" } else { "any_of" }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn test(&self, context: &mut LootContext) -> bool {
        for term in self.terms.holders(context.get_resolver()) {
            let passed = term.value(context.get_resolver()).test(context);
            if passed != self.all {
                return !self.all;
            }
        }
        self.all
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
        validate_holder_set(context, "terms", &self.terms);
    }
}

// ---- random -------------------------------------------------------------------------------------------------------------

/// `LootItemRandomChanceCondition`.
#[derive(Clone)]
pub struct LootItemRandomChanceCondition {
    pub chance: FloatHolder,
}

impl LootItemCondition for LootItemRandomChanceCondition {
    condition_basics!("random_chance");
    fn test(&self, context: &mut LootContext) -> bool {
        let probability = self.chance.value(context.get_resolver()).get_float(context);
        context.get_random().next_float() < probability
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
        validate_holder_field(context, "chance", &self.chance);
    }
}

/// `LootItemRandomChanceWithEnchantedBonusCondition`.
#[derive(Clone)]
pub struct LootItemRandomChanceWithEnchantedBonusCondition {
    pub unenchanted_chance: f32,
    pub enchanted_chance: LevelBasedValue,
    pub enchantment: Identifier,
}

impl LootItemCondition for LootItemRandomChanceWithEnchantedBonusCondition {
    condition_basics!("random_chance_with_enchanted_bonus");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::AttackingEntity]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let level = match context.params().attacking_entity {
            Some(killer) if killer.living().is_some() => enchant_api::get_enchantment_level(context.get_level(), &self.enchantment, killer),
            _ => 0,
        };
        let chance = if level > 0 { self.enchanted_chance.calculate(level) } else { self.unenchanted_chance };
        context.get_random().next_float() < chance
    }
}

/// `BonusLevelTableCondition`.
#[derive(Clone)]
pub struct BonusLevelTableCondition {
    pub enchantment: Identifier,
    pub values: Vec<f32>,
}

impl LootItemCondition for BonusLevelTableCondition {
    condition_basics!("table_bonus");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::Tool]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let level = context.params().tool.as_ref().map_or(0, |t| enchant_api::get_item_enchantment_level(&self.enchantment, t));
        let chance = self.values[(level.max(0) as usize).min(self.values.len() - 1)];
        // `values.get(Math.min(level, size - 1))`: a negative level is impossible for item enchantments
        context.get_random().next_float() < chance
    }
}

/// `ExplosionCondition`.
#[derive(Clone)]
pub struct ExplosionCondition;

impl LootItemCondition for ExplosionCondition {
    condition_basics!("survives_explosion");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::ExplosionRadius]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        match context.params().explosion_radius {
            Some(radius) => {
                let probability = 1.0f32 / radius;
                context.get_random().next_float() <= probability
            }
            None => true,
        }
    }
}

/// `LootItemKilledByPlayerCondition`.
#[derive(Clone)]
pub struct LootItemKilledByPlayerCondition;

impl LootItemCondition for LootItemKilledByPlayerCondition {
    condition_basics!("killed_by_player");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::LastDamagePlayer]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        context.has_parameter(ContextKey::LastDamagePlayer)
    }
}

// ---- entity / damage / location -------------------------------------------------------------------------------------------

/// `LootItemEntityPropertyCondition`.
#[derive(Clone)]
pub struct LootItemEntityPropertyCondition {
    pub predicate: Option<EntityPredicate>,
    pub entity_target: EntityTarget,
}

impl LootItemCondition for LootItemEntityPropertyCondition {
    condition_basics!("entity_properties");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::Origin, self.entity_target.context_param()]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let entity = self.entity_target.get(context);
        let origin = context.params().origin;
        match &self.predicate {
            None => true,
            Some(p) => entity.is_some_and(|e| p.matches(context.get_level(), origin, e)),
        }
    }
}

/// `DamageSourceCondition`.
#[derive(Clone)]
pub struct DamageSourceCondition {
    pub predicate: Option<DamageSourcePredicate>,
}

impl LootItemCondition for DamageSourceCondition {
    condition_basics!("damage_source_properties");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::Origin, ContextKey::DamageSource]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        match (context.params().damage_source, context.params().origin) {
            (Some(source), Some(pos)) => self.predicate.as_ref().is_none_or(|p| p.matches(context.get_level(), pos, source)),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Offset {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// `LocationCheck`.
#[derive(Clone)]
pub struct LocationCheck {
    pub predicate: Option<LocationPredicate>,
    pub offset: Offset,
}

impl LootItemCondition for LocationCheck {
    condition_basics!("location_check");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::Origin]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let Some(pos) = context.params().origin else { return false };
        self.predicate.as_ref().is_none_or(|p| {
            p.matches(context.get_level(), pos.x + f64::from(self.offset.x), pos.y + f64::from(self.offset.y), pos.z + f64::from(self.offset.z))
        })
    }
}

/// `EntityHasScoreCondition`.
#[derive(Clone)]
pub struct EntityHasScoreCondition {
    pub scores: Vec<(String, IntRangePredicate)>,
    pub entity_target: EntityTarget,
}

impl LootItemCondition for EntityHasScoreCondition {
    condition_basics!("entity_scores");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![self.entity_target.context_param()]
    }
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
        for (score, value) in &self.scores {
            value.validate(&context.for_map_field("scores", score));
        }
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let Some(entity) = self.entity_target.get(context) else { return false };
        let name = entity.get_scoreboard_name();
        for (objective, range) in &self.scores {
            match server_access::score(context.get_level(), &name, objective) {
                Some(v) if range.test(context, v) => {}
                _ => return false,
            }
        }
        true
    }
}

// ---- block / tool ---------------------------------------------------------------------------------------------------------

/// `MatchBlock`.
#[derive(Clone)]
pub struct MatchBlock {
    pub predicate: BlockPredicate,
}

impl LootItemCondition for MatchBlock {
    condition_basics!("match_block");
    fn referenced_params(&self) -> Vec<ContextKey> {
        if self.predicate.will_match_block_entity() {
            vec![ContextKey::BlockState, ContextKey::BlockEntity]
        } else {
            vec![ContextKey::BlockState]
        }
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let Some(state) = context.params().block_state else { return false };
        if !self.predicate.matches_state(state) {
            return false;
        }
        match context.params().block_entity {
            Some(be) => {
                let tag = be.save_with_full_metadata();
                let components = be.collect_components();
                self.predicate.matches_block_entity(Some((&tag, &components)))
            }
            None => self.predicate.matches_block_entity(None),
        }
    }
}

/// `MatchTool`.
#[derive(Clone)]
pub struct MatchTool {
    pub predicate: Option<ItemPredicate>,
}

impl LootItemCondition for MatchTool {
    condition_basics!("match_tool");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::Tool]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        match &context.params().tool {
            Some(tool) => self.predicate.as_ref().is_none_or(|p| p.test(tool)),
            None => false,
        }
    }
}

// ---- world ----------------------------------------------------------------------------------------------------------------

/// `WeatherCheck`.
#[derive(Clone)]
pub struct WeatherCheck {
    pub is_raining: Option<bool>,
    pub is_thundering: Option<bool>,
}

impl LootItemCondition for WeatherCheck {
    condition_basics!("weather_check");
    fn test(&self, context: &mut LootContext) -> bool {
        let level = context.get_level();
        if self.is_raining.is_some_and(|r| r != level.is_raining()) {
            return false;
        }
        self.is_thundering.is_none_or(|t| t == level.is_thundering())
    }
}

/// `TimeCheck`.
#[derive(Clone)]
pub struct TimeCheck {
    pub clock: Identifier,
    pub period: Option<i64>,
    pub value: IntRangePredicate,
}

impl LootItemCondition for TimeCheck {
    condition_basics!("time_check");
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
        self.value.validate(&context.for_field("value"));
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let mut time = server_access::clock_total_ticks(context.get_level(), &self.clock);
        if let Some(period) = self.period {
            // Java `long %`: a zero period is an `ArithmeticException` in vanilla; no such table loads without error there either
            time = time.wrapping_rem(period);
        }
        self.value.test(context, time as i32)
    }
}

/// `IntValueCheck`.
#[derive(Clone)]
pub struct IntValueCheck {
    pub value: IntHolder,
    pub range: IntRangePredicate,
}

impl LootItemCondition for IntValueCheck {
    condition_basics!("int_value_check");
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
        validate_holder_field(context, "value", &self.value);
        self.range.validate(&context.for_field("range"));
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let range = self.range.clone();
        range.test_provider(context, &self.value)
    }
}

/// `FloatValueCheck`.
#[derive(Clone)]
pub struct FloatValueCheck {
    pub value: FloatHolder,
    pub range: FloatRangePredicate,
}

impl LootItemCondition for FloatValueCheck {
    condition_basics!("float_value_check");
    fn validate(&self, context: &ValidationContext) {
        context.validate_context_usage(&self.referenced_params());
        validate_holder_field(context, "value", &self.value);
        self.range.validate(&context.for_field("range"));
    }
    fn test(&self, context: &mut LootContext) -> bool {
        let range = self.range.clone();
        range.test_provider(context, &self.value)
    }
}

/// `EnchantmentActiveCheck`.
#[derive(Clone)]
pub struct EnchantmentActiveCheck {
    pub active: bool,
}

impl LootItemCondition for EnchantmentActiveCheck {
    condition_basics!("enchantment_active_check");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::EnchantmentActive]
    }
    fn test(&self, context: &mut LootContext) -> bool {
        context.params().enchantment_active == Some(self.active)
    }
}

/// `EnvironmentAttributeCheck`.
#[derive(Clone)]
pub struct EnvironmentAttributeCheck {
    pub attribute: Identifier,
    pub value: crate::world::attribute::AttrValue,
}

impl LootItemCondition for EnvironmentAttributeCheck {
    condition_basics!("environment_attribute_check");
    fn referenced_params(&self) -> Vec<ContextKey> {
        if server_access::environment_attribute_is_positional(&self.attribute) { vec![ContextKey::Origin] } else { vec![] }
    }
    fn test(&self, context: &mut LootContext) -> bool {
        server_access::environment_attribute_matches(context, &self.attribute, &self.value, context.params().origin)
    }
}

// ---- codecs ---------------------------------------------------------------------------------------------------------------

pub(super) fn type_table() -> Arc<TypeTable<dyn LootItemCondition>> {
    fn entry<C: LootItemCondition + Clone + 'static>(codec: MapCodec<C>) -> MapCodec<ConditionRef> {
        lift(codec, up::<C>, |t| t.as_any().downcast_ref::<C>())
    }
    let float_h = float_holder_codec;
    TypeTable::new(
        "minecraft:loot_condition_type",
        vec![
            ("inverted", entry(record!(InvertedLootItemCondition { term: condition_holder_codec().field_of("term") }))),
            (
                "any_of",
                entry(
                    condition_list_codec()
                        .field_of("terms")
                        .xmap(|terms| CompositeLootItemCondition { all: false, terms }, |c: &CompositeLootItemCondition| c.terms.clone()),
                ),
            ),
            (
                "all_of",
                entry(
                    condition_list_codec()
                        .field_of("terms")
                        .xmap(|terms| CompositeLootItemCondition { all: true, terms }, |c: &CompositeLootItemCondition| c.terms.clone()),
                ),
            ),
            ("random_chance", entry(record!(LootItemRandomChanceCondition { chance: float_h().field_of("chance") }))),
            (
                "random_chance_with_enchanted_bonus",
                entry(record!(LootItemRandomChanceWithEnchantedBonusCondition {
                    unenchanted_chance: float_range(0.0, 1.0).field_of("unenchanted_chance"),
                    enchanted_chance: LevelBasedValue::codec().field_of("enchanted_chance"),
                    enchantment: registry_ref_codec(keys::ENCHANTMENT).field_of("enchantment"),
                })),
            ),
            (
                "entity_properties",
                entry(record!(LootItemEntityPropertyCondition {
                    predicate: EntityPredicate::codec().optional_field_of("predicate"),
                    entity_target: EntityTarget::codec().field_of("entity"),
                })),
            ),
            ("killed_by_player", entry(MapCodec::unit(LootItemKilledByPlayerCondition))),
            (
                "entity_scores",
                entry(record!(EntityHasScoreCondition {
                    scores: STRING.unbounded_map(&IntRangePredicate::codec()).field_of("scores"),
                    entity_target: EntityTarget::codec().field_of("entity"),
                })),
            ),
            ("match_block", entry(BlockPredicate::map_codec().xmap(|predicate| MatchBlock { predicate }, |m: &MatchBlock| m.predicate.clone()))),
            ("match_tool", entry(record!(MatchTool { predicate: ItemPredicate::codec().optional_field_of("predicate") }))),
            (
                "table_bonus",
                entry(record!(BonusLevelTableCondition {
                    enchantment: registry_ref_codec(keys::ENCHANTMENT).field_of("enchantment"),
                    values: non_empty_list(&FLOAT.list_of()).field_of("chances"),
                })),
            ),
            ("survives_explosion", entry(MapCodec::unit(ExplosionCondition))),
            ("damage_source_properties", entry(record!(DamageSourceCondition { predicate: DamageSourcePredicate::codec().optional_field_of("predicate") }))),
            (
                "location_check",
                entry(record!(LocationCheck {
                    predicate: LocationPredicate::codec().optional_field_of("predicate"),
                    offset: record!(Offset {
                        x: INT.optional_field_of_default("offsetX", 0),
                        y: INT.optional_field_of_default("offsetY", 0),
                        z: INT.optional_field_of_default("offsetZ", 0),
                    }),
                })),
            ),
            (
                "weather_check",
                entry(record!(WeatherCheck { is_raining: BOOL.optional_field_of("raining"), is_thundering: BOOL.optional_field_of("thundering") })),
            ),
            (
                "time_check",
                entry(record!(TimeCheck {
                    clock: registry_ref_codec(keys::WORLD_CLOCK).field_of("clock"),
                    period: LONG.optional_field_of("period"),
                    value: IntRangePredicate::codec().field_of("value"),
                })),
            ),
            (
                "int_value_check",
                entry(record!(IntValueCheck { value: int_holder_codec().field_of("value"), range: IntRangePredicate::codec().field_of("test") })),
            ),
            (
                "float_value_check",
                entry(record!(FloatValueCheck { value: float_h().field_of("value"), range: FloatRangePredicate::codec().field_of("test") })),
            ),
            ("enchantment_active_check", entry(record!(EnchantmentActiveCheck { active: BOOL.field_of("active") }))),
            ("environment_attribute_check", entry(environment_attribute_check_codec())),
        ],
    )
}

fn environment_attribute_check_codec() -> MapCodec<EnvironmentAttributeCheck> {
    server_access::environment_attribute_id_codec().partial_dispatch_map(
        "attribute",
        |c: &EnvironmentAttributeCheck| DataResult::success(c.attribute.clone()),
        |attribute: &Identifier| match server_access::environment_attribute_value_codec(attribute) {
            Some(value_codec) => {
                let a = attribute.clone();
                DataResult::success(value_codec.field_of("value").xmap(
                    move |value| EnvironmentAttributeCheck { attribute: a.clone(), value },
                    |c: &EnvironmentAttributeCheck| c.value,
                ))
            }
            None => {
                let m = format!("Unsupported environment attribute type for {attribute}");
                DataResult::error(move || m.clone())
            }
        },
    )
}
