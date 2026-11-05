//! The common loot functions: counts, damage, bonus/decay, components, custom data, names, lore, sequence/filtered,
//! block state / component copies.

use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_core::math::java::round_f32;
use mc_core::math::mth;
use mc_data::block::Block;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{BOOL, Codec, DataResult, FLOAT, INT, LONG, MapCodec, STRING, record};
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::values::basic::{CUSTOM_DATA, CUSTOM_NAME, ITEM_NAME, LORE, TOOLTIP_DISPLAY};
use mc_protocol::components::values::containers::{BLOCK_STATE, CONTAINER_LOOT, SeededContainerLoot};
use mc_protocol::components::values::predicates::ItemPredicate;
use mc_protocol::components::{CustomData, DataComponentMap, DataComponentPatch, ItemStack};
use mc_text::Component;

use super::list_operation::ListOperation;
use super::{
    ConditionalFunction, FunctionHolder, FunctionHolderSet, LootItemFunctionRef, condition_field, function_holder_codec, function_list_codec_typed,
};
use crate::world::level::storage::loot::codec_ext::dispatch_optional_value;
use crate::world::level::storage::loot::context::{ContextKey, EntityTarget, LootContext};
use crate::world::level::storage::loot::enchant_api;
use crate::world::level::storage::loot::holder::{lift, registry_ref_codec};
use crate::world::level::storage::loot::predicates::ConditionHolder;
use crate::world::level::storage::loot::providers::nbt::{NbtProvider, nbt_path_codec, nbt_provider_codec};
use crate::world::level::storage::loot::providers::number::{FloatHolder, IntHolder, float_holder_codec, int_holder_codec};
use crate::world::level::storage::loot::range::IntLimit;
use crate::world::level::storage::loot::validation::{ValidationContext, validate_holder_field, validate_holder_opt};
use crate::registry::keys;

type Entries = Vec<(&'static str, MapCodec<LootItemFunctionRef>)>;

fn entry<F: ConditionalFunction + Clone>(codec: MapCodec<F>) -> MapCodec<LootItemFunctionRef> {
    lift(codec, |f| Arc::new(f) as LootItemFunctionRef, |t| t.as_any().downcast_ref::<F>())
}

macro_rules! conditional {
    ($ty:ty, $name:literal) => {
        fn type_name(&self) -> &'static str {
            $name
        }
        fn condition(&self) -> &Option<ConditionHolder> {
            &self.condition
        }
    };
}

// ---- set_count / set_item / set_damage / limit_count --------------------------------------------------------------------

#[derive(Clone)]
pub struct SetItemCountFunction {
    pub condition: Option<ConditionHolder>,
    pub count: IntHolder,
    pub add: bool,
}

impl ConditionalFunction for SetItemCountFunction {
    conditional!(SetItemCountFunction, "set_count");
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let base = if self.add { stack.count() } else { 0 };
        let count = self.count.value(context.get_resolver()).get_int(context);
        stack.set_count(base.wrapping_add(count));
        stack
    }
    fn validate_fields(&self, context: &ValidationContext) {
        validate_holder_field(context, "count", &self.count);
    }
}

#[derive(Clone)]
pub struct SetItemFunction {
    pub condition: Option<ConditionHolder>,
    pub item: mc_data::Item,
}

impl ConditionalFunction for SetItemFunction {
    conditional!(SetItemFunction, "set_item");
    fn run(&self, stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        // `ItemStack.transmuteCopy(item)`: same count and component patch on the new item
        if stack.is_empty() { stack } else { ItemStack::with_patch(self.item, stack.count(), &stack.get_components_patch()) }
    }
}

#[derive(Clone)]
pub struct SetItemDamageFunction {
    pub condition: Option<ConditionHolder>,
    pub damage: FloatHolder,
    pub add: bool,
}

impl ConditionalFunction for SetItemDamageFunction {
    conditional!(SetItemDamageFunction, "set_damage");
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if stack.is_damageable_item() {
            let max_damage = stack.get_max_damage();
            let base = if self.add { 1.0f32 - stack.get_damage_value() as f32 / max_damage as f32 } else { 0.0 };
            let damage = self.damage.value(context.get_resolver()).get_float(context);
            let pct = 1.0f32 - mth::clamp_f32(damage + base, 0.0, 1.0);
            stack.set_damage_value(mth::floor_f32(pct * max_damage as f32));
        } else {
            log::warn!("Couldn't set damage of loot item {stack:?}");
        }
        stack
    }
    fn validate_fields(&self, context: &ValidationContext) {
        validate_holder_field(context, "damage", &self.damage);
    }
}

#[derive(Clone)]
pub struct LimitCount {
    pub condition: Option<ConditionHolder>,
    pub limit: IntLimit,
}

impl ConditionalFunction for LimitCount {
    conditional!(LimitCount, "limit_count");
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let new_count = self.limit.clamp(context, stack.count());
        stack.set_count(new_count);
        stack
    }
    fn validate_fields(&self, context: &ValidationContext) {
        self.limit.validate(&context.for_field("limit"));
    }
}

// ---- apply_bonus / explosion_decay / enchanted_count_increase ---------------------------------------------------------------

/// `ApplyBonusCount.Formula`.
#[derive(Clone, PartialEq, Debug)]
pub enum BonusFormula {
    BinomialWithBonusCount { extra_rounds: i32, probability: f32 },
    OreDrops,
    UniformBonusCount { bonus_multiplier: i32 },
}

impl BonusFormula {
    fn id(&self) -> &'static str {
        match self {
            BonusFormula::BinomialWithBonusCount { .. } => "binomial_with_bonus_count",
            BonusFormula::OreDrops => "ore_drops",
            BonusFormula::UniformBonusCount { .. } => "uniform_bonus_count",
        }
    }

    fn calculate_new_count(&self, random: &mut dyn mc_core::random::RandomSource, mut count: i32, level: i32) -> i32 {
        match self {
            BonusFormula::BinomialWithBonusCount { extra_rounds, probability } => {
                let rounds = level.wrapping_add(*extra_rounds);
                for _ in 0..rounds {
                    if random.next_float() < *probability {
                        count = count.wrapping_add(1);
                    }
                }
                count
            }
            BonusFormula::OreDrops => {
                if level > 0 {
                    let bonus = (random.next_int_bounded(level + 2) - 1).max(0);
                    count.wrapping_mul(bonus + 1)
                } else {
                    count
                }
            }
            BonusFormula::UniformBonusCount { bonus_multiplier } => {
                count.wrapping_add(random.next_int_bounded(bonus_multiplier.wrapping_mul(level).wrapping_add(1)))
            }
        }
    }

    fn codec() -> MapCodec<BonusFormula> {
        let id_codec: Codec<&'static str> = IDENTIFIER.flat_xmap(
            |id: Identifier| {
                let name = (id.namespace() == "minecraft")
                    .then(|| ["binomial_with_bonus_count", "ore_drops", "uniform_bonus_count"].into_iter().find(|n| *n == id.path()))
                    .flatten();
                match name {
                    Some(n) => DataResult::success(n),
                    None => DataResult::error(move || format!("No formula type with id: '{id}'")),
                }
            },
            |name: &&'static str| DataResult::success(Identifier::new("minecraft", name).expect("valid")),
        );
        dispatch_optional_value(
            "formula",
            "parameters",
            id_codec,
            |f: &BonusFormula| f.id(),
            |id: &&'static str| {
                DataResult::success(match *id {
                    "binomial_with_bonus_count" => record!(Binomial { extra: INT.field_of("extra"), probability: FLOAT.field_of("probability") })
                        .xmap(
                            |b| BonusFormula::BinomialWithBonusCount { extra_rounds: b.extra, probability: b.probability },
                            |f: &BonusFormula| match f {
                                BonusFormula::BinomialWithBonusCount { extra_rounds, probability } => Binomial { extra: *extra_rounds, probability: *probability },
                                _ => Binomial { extra: 0, probability: 0.0 },
                            },
                        )
                        .codec(),
                    "ore_drops" => Codec::unit(BonusFormula::OreDrops),
                    _ => record!(Uniform { bonus_multiplier: INT.field_of("bonusMultiplier") })
                        .xmap(
                            |u| BonusFormula::UniformBonusCount { bonus_multiplier: u.bonus_multiplier },
                            |f: &BonusFormula| match f {
                                BonusFormula::UniformBonusCount { bonus_multiplier } => Uniform { bonus_multiplier: *bonus_multiplier },
                                _ => Uniform { bonus_multiplier: 0 },
                            },
                        )
                        .codec(),
                })
            },
        )
    }
}

#[derive(Clone)]
struct Binomial {
    extra: i32,
    probability: f32,
}
#[derive(Clone)]
struct Uniform {
    bonus_multiplier: i32,
}

#[derive(Clone)]
pub struct ApplyBonusCount {
    pub condition: Option<ConditionHolder>,
    pub enchantment: Identifier,
    pub formula: BonusFormula,
}

impl ConditionalFunction for ApplyBonusCount {
    conditional!(ApplyBonusCount, "apply_bonus");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::Tool]
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if let Some(tool) = context.params().tool.clone() {
            let level = enchant_api::get_item_enchantment_level(&self.enchantment, &tool);
            let new_count = self.formula.calculate_new_count(context.get_random(), stack.count(), level);
            stack.set_count(new_count);
        }
        stack
    }
}

#[derive(Clone)]
pub struct ApplyExplosionDecay {
    pub condition: Option<ConditionHolder>,
}

impl ConditionalFunction for ApplyExplosionDecay {
    conditional!(ApplyExplosionDecay, "explosion_decay");
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if let Some(radius) = context.params().explosion_radius {
            let random = context.get_random();
            let probability = 1.0f32 / radius;
            let current = stack.count();
            let mut result = 0;
            for _ in 0..current {
                if random.next_float() <= probability {
                    result += 1;
                }
            }
            stack.set_count(result);
        }
        stack
    }
}

#[derive(Clone)]
pub struct EnchantedCountIncreaseFunction {
    pub condition: Option<ConditionHolder>,
    pub enchantment: Identifier,
    pub count: FloatHolder,
    pub limit: i32,
}

impl ConditionalFunction for EnchantedCountIncreaseFunction {
    conditional!(EnchantedCountIncreaseFunction, "enchanted_count_increase");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::AttackingEntity]
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if let Some(killer) = context.params().attacking_entity {
            if killer.living().is_some() {
                let level = enchant_api::get_enchantment_level(context.get_level(), &self.enchantment, killer);
                if level == 0 {
                    return stack;
                }
                let addition = level as f32 * self.count.value(context.get_resolver()).get_float(context);
                stack.grow(round_f32(addition));
                if self.limit > 0 {
                    stack.limit_size(self.limit);
                }
            }
        }
        stack
    }
    fn validate_fields(&self, context: &ValidationContext) {
        validate_holder_field(context, "count", &self.count);
    }
}

// ---- smelting ---------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct SmeltItemFunction {
    pub condition: Option<ConditionHolder>,
    pub use_input_count: bool,
}

impl ConditionalFunction for SmeltItemFunction {
    conditional!(SmeltItemFunction, "furnace_smelt");
    fn run(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if stack.is_empty() {
            return stack;
        }
        match crate::world::level::storage::loot::recipe_api::smelting_result(context.get_level(), &stack) {
            Some(result) if !result.is_empty() => {
                let new_count = (if self.use_input_count { stack.count() } else { 1 }).wrapping_mul(result.count());
                result.copy_with_count(new_count.min(result.get_max_stack_size()))
            }
            _ => {
                log::warn!("Couldn't smelt {stack:?} because there is no smelting recipe");
                stack
            }
        }
    }
}

// ---- components / custom data ------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct SetComponentsFunction {
    pub condition: Option<ConditionHolder>,
    pub components: DataComponentPatch,
}

impl ConditionalFunction for SetComponentsFunction {
    conditional!(SetComponentsFunction, "set_components");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        // `applyComponentsAndValidate`: invalid results are logged and the original components kept
        let before = stack.clone();
        stack.apply_components(&self.components);
        if let Err(e) = ItemStack::validate_strict(stack.clone()).into_result() {
            log::warn!("Failed to validate components of loot item: {e}");
            return before;
        }
        stack
    }
}

#[derive(Clone)]
pub struct SetCustomDataFunction {
    pub condition: Option<ConditionHolder>,
    pub tag: CompoundTag,
}

impl ConditionalFunction for SetCustomDataFunction {
    conditional!(SetCustomDataFunction, "set_custom_data");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let mut tag = stack.get(&CUSTOM_DATA).map_or_else(CompoundTag::new, CustomData::copy_tag);
        tag.merge(&self.tag);
        stack.set(CUSTOM_DATA, CustomData::of(&tag));
        stack
    }
}

#[derive(Clone)]
pub struct CopyNameFunction {
    pub condition: Option<ConditionHolder>,
    pub source: NameSource,
}

/// `LootContextArg.ENTITY_OR_BLOCK` source of names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameSource {
    Entity(EntityTarget),
    BlockEntity,
}

impl NameSource {
    pub fn codec() -> Codec<NameSource> {
        static ALL: LazyLock<Vec<(NameSource, &'static str)>> = LazyLock::new(|| {
            let mut all: Vec<(NameSource, &'static str)> = EntityTarget::ALL.iter().map(|t| (NameSource::Entity(*t), t.name())).collect();
            all.push((NameSource::BlockEntity, "block_entity"));
            all
        });
        mc_dfu::string_enum_codec(LazyLock::force(&ALL).as_slice())
    }

    fn context_param(self) -> ContextKey {
        match self {
            NameSource::Entity(t) => t.context_param(),
            NameSource::BlockEntity => ContextKey::BlockEntity,
        }
    }
}

impl ConditionalFunction for CopyNameFunction {
    conditional!(CopyNameFunction, "copy_name");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![self.source.context_param()]
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let name: Option<Option<Component>> = match self.source {
            NameSource::Entity(target) => target.get(context).map(|e| e.base().get_custom_name()),
            NameSource::BlockEntity => context.params().block_entity.map(|be| be.get_custom_name()),
        };
        // `itemStack.set(CUSTOM_NAME, nameable.getCustomName())` (a null name removes the component)
        match name {
            Some(Some(n)) => {
                stack.set(CUSTOM_NAME, n);
            }
            Some(None) => {
                stack.remove(mc_data::DataComponentType::CUSTOM_NAME);
            }
            None => {}
        }
        stack
    }
}

#[derive(Clone)]
pub struct DiscardItem {
    pub condition: Option<ConditionHolder>,
}

impl ConditionalFunction for DiscardItem {
    conditional!(DiscardItem, "discard");
    fn run(&self, _stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        ItemStack::empty()
    }
}

#[derive(Clone)]
pub struct ToggleTooltips {
    pub condition: Option<ConditionHolder>,
    pub values: Vec<(mc_data::DataComponentType, bool)>,
}

impl ConditionalFunction for ToggleTooltips {
    conditional!(ToggleTooltips, "toggle_tooltips");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        let mut display = stack.get(&TOOLTIP_DISPLAY).cloned().unwrap_or_default();
        for (component, shown) in &self.values {
            let hidden = !*shown;
            if display.hidden_components.contains(component) != hidden {
                if hidden {
                    display.hidden_components.push(*component);
                } else {
                    display.hidden_components.retain(|c| c != component);
                }
            }
        }
        stack.set(TOOLTIP_DISPLAY, display);
        stack
    }
}

// ---- names / lore --------------------------------------------------------------------------------------------------------------

/// `SetNameFunction.Target`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameTarget {
    CustomName,
    ItemName,
}

const NAME_TARGETS: &[(NameTarget, &str)] = &[(NameTarget::CustomName, "custom_name"), (NameTarget::ItemName, "item_name")];

#[derive(Clone)]
pub struct SetNameFunction {
    pub condition: Option<ConditionHolder>,
    pub name: Option<Component>,
    pub resolution_context: Option<EntityTarget>,
    pub target: NameTarget,
}

/// `SetNameFunction.createResolver(context, entityTarget)`.
pub fn resolve_component(context: &LootContext, entity_target: Option<EntityTarget>, line: &Component) -> Component {
    if let Some(entity) = entity_target.and_then(|t| t.get(context)) {
        match crate::world::level::storage::loot::text_api::resolve_component(context.get_level(), entity, line) {
            Ok(c) => return c,
            Err(e) => log::warn!("Failed to resolve text component: {e}"),
        }
    }
    line.clone()
}

impl ConditionalFunction for SetNameFunction {
    conditional!(SetNameFunction, "set_name");
    fn referenced_params(&self) -> Vec<ContextKey> {
        self.resolution_context.map(|t| vec![t.context_param()]).unwrap_or_default()
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if let Some(name) = &self.name {
            let resolved = resolve_component(context, self.resolution_context, name);
            match self.target {
                NameTarget::CustomName => stack.set(CUSTOM_NAME, resolved),
                NameTarget::ItemName => stack.set(ITEM_NAME, resolved),
            };
        }
        stack
    }
}

#[derive(Clone)]
pub struct SetLoreFunction {
    pub condition: Option<ConditionHolder>,
    pub lore: Vec<Component>,
    pub mode: ListOperation,
    pub resolution_context: Option<EntityTarget>,
}

impl ConditionalFunction for SetLoreFunction {
    conditional!(SetLoreFunction, "set_lore");
    fn referenced_params(&self) -> Vec<ContextKey> {
        self.resolution_context.map(|t| vec![t.context_param()]).unwrap_or_default()
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let old = stack.get(&LORE).cloned().unwrap_or_default();
        let lines = if old.lines().is_empty() && self.lore.is_empty() {
            Vec::new()
        } else {
            let resolved: Vec<Component> = self.lore.iter().map(|l| resolve_component(context, self.resolution_context, l)).collect();
            self.mode.apply(old.lines(), &resolved, 256)
        };
        stack.set(LORE, mc_protocol::components::values::basic::ItemLore::new(lines));
        stack
    }
}

// ---- sequence / filtered -------------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct SequenceFunction {
    condition: Option<ConditionHolder>,
    functions: FunctionHolderSet,
}

impl SequenceFunction {
    pub fn new(condition: Option<ConditionHolder>, functions: FunctionHolderSet) -> SequenceFunction {
        SequenceFunction { condition, functions }
    }
    pub fn functions(&self) -> &FunctionHolderSet {
        &self.functions
    }
    pub fn can_use_inline_codec(&self) -> bool {
        self.condition.is_none()
    }
}

impl ConditionalFunction for SequenceFunction {
    conditional!(SequenceFunction, "sequence");
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        for function in self.functions.holders(context.get_resolver()) {
            stack = function.value(context.get_resolver()).apply(stack, context);
        }
        stack
    }
    fn validate_fields(&self, context: &ValidationContext) {
        crate::world::level::storage::loot::validation::validate_holder_set(context, "functions", &self.functions);
    }
}

#[derive(Clone)]
pub struct FilteredFunction {
    pub condition: Option<ConditionHolder>,
    pub filter: ItemPredicate,
    pub on_pass: Option<FunctionHolder>,
    pub on_fail: Option<FunctionHolder>,
}

impl ConditionalFunction for FilteredFunction {
    conditional!(FilteredFunction, "filtered");
    fn run(&self, stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let function = if self.filter.test(&stack) { &self.on_pass } else { &self.on_fail };
        match function {
            Some(f) => f.value(context.get_resolver()).apply(stack, context),
            None => stack,
        }
    }
    fn validate_fields(&self, context: &ValidationContext) {
        validate_holder_opt(context, "on_pass", &self.on_pass);
        validate_holder_opt(context, "on_fail", &self.on_fail);
    }
}

// ---- copy state / components / custom data ---------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct CopyBlockState {
    pub condition: Option<ConditionHolder>,
    pub block: Block,
    /// Property names that exist on `block` (unknown names are dropped like `getProperty(name) == null`).
    pub properties: Vec<String>,
}

impl ConditionalFunction for CopyBlockState {
    conditional!(CopyBlockState, "copy_state");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![ContextKey::BlockState]
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if let Some(state) = context.params().block_state {
            let mut item_state = stack.get(&BLOCK_STATE).cloned().unwrap_or_default();
            for (def, index) in state.values() {
                if self.properties.iter().any(|p| p == def.name) {
                    let value = def.values[index].to_owned();
                    match item_state.properties.iter_mut().find(|(k, _)| k == def.name) {
                        Some(e) => e.1 = value,
                        None => item_state.properties.push((def.name.to_owned(), value)),
                    }
                }
            }
            stack.set(BLOCK_STATE, item_state);
        }
        stack
    }
}

/// `CopyComponentsFunction` source.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ComponentSource {
    BlockEntity,
    Tool,
}

#[derive(Clone)]
pub struct CopyComponentsFunction {
    pub condition: Option<ConditionHolder>,
    pub source: ComponentSource,
    pub include: Option<Vec<mc_data::DataComponentType>>,
    pub exclude: Option<Vec<mc_data::DataComponentType>>,
}

impl ConditionalFunction for CopyComponentsFunction {
    conditional!(CopyComponentsFunction, "copy_components");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![match self.source {
            ComponentSource::BlockEntity => ContextKey::BlockEntity,
            ComponentSource::Tool => ContextKey::Tool,
        }]
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let allowed = |t: mc_data::DataComponentType| {
            self.exclude.as_ref().is_none_or(|e| !e.contains(&t)) && self.include.as_ref().is_none_or(|i| i.contains(&t))
        };
        match self.source {
            ComponentSource::BlockEntity => {
                if let Some(be) = context.params().block_entity {
                    let components: DataComponentMap = be.collect_components();
                    for (t, v) in components.iter() {
                        if allowed(t) {
                            stack.set_value(t, Some(v.clone()));
                        }
                    }
                }
            }
            ComponentSource::Tool => {
                if let Some(tool) = context.params().tool.clone() {
                    let exclude = self.exclude.clone().unwrap_or_default();
                    let types: Vec<mc_data::DataComponentType> = match &self.include {
                        Some(i) => i.clone(),
                        None => mc_data::DataComponentType::iter().collect(),
                    };
                    for t in types {
                        if !exclude.contains(&t) {
                            if let Some(v) = tool.get_value(t) {
                                stack.set_value(t, Some(v.clone()));
                            }
                        }
                    }
                }
            }
        }
        stack
    }
}

/// `CopyCustomDataFunction.MergeStrategy`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MergeStrategy {
    Replace,
    Append,
    Merge,
}

const MERGE_STRATEGIES: &[(MergeStrategy, &str)] =
    &[(MergeStrategy::Replace, "replace"), (MergeStrategy::Append, "append"), (MergeStrategy::Merge, "merge")];

#[derive(Clone)]
pub struct CopyOperation {
    pub source_path: Arc<crate::commands::arguments::nbt_path_argument::NbtPath>,
    pub target_path: Arc<crate::commands::arguments::nbt_path_argument::NbtPath>,
    pub op: MergeStrategy,
}

impl MergeStrategy {
    fn merge(
        self,
        target: &mut Tag,
        path: &crate::commands::arguments::nbt_path_argument::NbtPath,
        sources: &[Tag],
    ) -> Result<(), mc_brigadier::CommandSyntaxException> {
        match self {
            MergeStrategy::Replace => {
                path.set(target, sources.last().expect("non-empty"))?;
            }
            MergeStrategy::Append => {
                path.get_or_create_mut(target, &|| Tag::List(mc_nbt::ListTag::new()), &mut |tag| {
                    if let Tag::List(list) = tag {
                        for s in sources {
                            list.push(s.clone());
                        }
                    }
                    Ok(())
                })?;
            }
            MergeStrategy::Merge => {
                path.get_or_create_mut(target, &|| Tag::Compound(CompoundTag::new()), &mut |tag| {
                    if let Tag::Compound(compound) = tag {
                        for s in sources {
                            if let Tag::Compound(src) = s {
                                compound.merge(src);
                            }
                        }
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct CopyCustomDataFunction {
    pub condition: Option<ConditionHolder>,
    pub source: Arc<dyn NbtProvider>,
    pub operations: Vec<CopyOperation>,
}

impl ConditionalFunction for CopyCustomDataFunction {
    conditional!(CopyCustomDataFunction, "copy_custom_data");
    fn validate_fields(&self, context: &ValidationContext) {
        self.source.validate(&context.for_field("source"));
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        let Some(source_tag) = self.source.get(context) else { return stack };
        let mut result: Option<Tag> = None;
        for op in &self.operations {
            if let Ok(source_tags) = op.source_path.get(&source_tag) {
                if !source_tags.is_empty() {
                    let target = result
                        .get_or_insert_with(|| Tag::Compound(stack.get(&CUSTOM_DATA).map_or_else(CompoundTag::new, CustomData::copy_tag)));
                    let _ = op.op.merge(target, &op.target_path, &source_tags);
                }
            }
        }
        if let Some(Tag::Compound(tag)) = result {
            stack.set(CUSTOM_DATA, CustomData::of(&tag));
        }
        stack
    }
}

// ---- fill player head / container loot table ---------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct FillPlayerHead {
    pub condition: Option<ConditionHolder>,
    pub entity_target: EntityTarget,
}

impl ConditionalFunction for FillPlayerHead {
    conditional!(FillPlayerHead, "fill_player_head");
    fn referenced_params(&self) -> Vec<ContextKey> {
        vec![self.entity_target.context_param()]
    }
    fn run(&self, mut stack: ItemStack, context: &mut LootContext) -> ItemStack {
        if stack.is(mc_data::Item::PLAYER_HEAD) {
            if let Some(donor) = self.entity_target.get(context).filter(|e| e.is_player()) {
                if let Some(profile) = crate::world::level::storage::loot::text_api::player_profile(donor) {
                    stack.set(mc_protocol::components::values::profile::PROFILE, profile);
                }
            }
        }
        stack
    }
}

#[derive(Clone)]
pub struct SetContainerLootTable {
    pub condition: Option<ConditionHolder>,
    pub loot_table_id: Identifier,
    pub seed: i64,
}

impl ConditionalFunction for SetContainerLootTable {
    conditional!(SetContainerLootTable, "set_loot_table");
    fn run(&self, mut stack: ItemStack, _context: &mut LootContext) -> ItemStack {
        if stack.is_empty() {
            return stack;
        }
        stack.set(CONTAINER_LOOT, SeededContainerLoot { loot_table: self.loot_table_id.clone(), seed: self.seed });
        stack
    }
}

// ---- registration ----------------------------------------------------------------------------------------------------------------

fn name_target_codec() -> Codec<NameTarget> {
    mc_dfu::string_enum_codec(NAME_TARGETS)
}

pub(super) fn register(entries: &mut Entries) {
    let cond = condition_field;
    entries.push((
        "set_count",
        entry(record!(SetItemCountFunction {
            condition: cond(),
            count: int_holder_codec().field_of("count"),
            add: BOOL.optional_field_of_default("add", false),
        })),
    ));
    entries.push((
        "set_item",
        entry(record!(SetItemFunction {
            condition: cond(),
            item: mc_protocol::components::holder::registry_by_name_codec::<mc_data::Item>().field_of("item"),
        })),
    ));
    entries.push((
        "set_damage",
        entry(record!(SetItemDamageFunction {
            condition: cond(),
            damage: float_holder_codec().field_of("damage"),
            add: BOOL.optional_field_of_default("add", false),
        })),
    ));
    entries.push(("limit_count", entry(record!(LimitCount { condition: cond(), limit: IntLimit::codec().field_of("limit") }))));
    entries.push((
        "apply_bonus",
        entry(record!(ApplyBonusCount {
            condition: cond(),
            enchantment: registry_ref_codec(keys::ENCHANTMENT).field_of("enchantment"),
            formula: BonusFormula::codec(),
        })),
    ));
    entries.push(("explosion_decay", entry(record!(ApplyExplosionDecay { condition: cond() }))));
    entries.push((
        "enchanted_count_increase",
        entry(record!(EnchantedCountIncreaseFunction {
            condition: cond(),
            enchantment: registry_ref_codec(keys::ENCHANTMENT).field_of("enchantment"),
            count: float_holder_codec().field_of("count"),
            limit: INT.optional_field_of_default("limit", 0),
        })),
    ));
    entries.push((
        "furnace_smelt",
        entry(record!(SmeltItemFunction { condition: cond(), use_input_count: BOOL.optional_field_of_default("use_input_count", true) })),
    ));
    entries.push((
        "set_components",
        entry(record!(SetComponentsFunction { condition: cond(), components: DataComponentPatch::codec().field_of("components") })),
    ));
    entries.push((
        "set_custom_data",
        entry(record!(SetCustomDataFunction {
            condition: cond(),
            tag: mc_protocol::components::values::predicates::NbtPredicate::codec()
                .xmap(|p| p.tag, |t: &CompoundTag| mc_protocol::components::values::predicates::NbtPredicate { tag: t.clone() })
                .field_of("tag"),
        })),
    ));
    entries.push(("copy_name", entry(record!(CopyNameFunction { condition: cond(), source: NameSource::codec().field_of("source") }))));
    entries.push(("discard", entry(record!(DiscardItem { condition: cond() }))));
    entries.push((
        "toggle_tooltips",
        entry(record!(ToggleTooltips {
            condition: cond(),
            values: mc_protocol::components::map::type_codec().unbounded_map(&BOOL).field_of("toggles"),
        })),
    ));
    entries.push((
        "set_name",
        entry(record!(SetNameFunction {
            condition: cond(),
            name: mc_protocol::components::component_codec().optional_field_of("name"),
            resolution_context: EntityTarget::codec().optional_field_of("entity"),
            target: name_target_codec().optional_field_of_default("target", NameTarget::CustomName),
        })),
    ));
    entries.push((
        "set_lore",
        entry(record!(SetLoreFunction {
            condition: cond(),
            lore: mc_protocol::components::component_codec().size_limited_list_of(256).field_of("lore"),
            mode: ListOperation::codec(256),
            resolution_context: EntityTarget::codec().optional_field_of("entity"),
        })),
    ));
    entries.push((
        "sequence",
        entry(record!(SequenceFunction { condition: cond(), functions: function_list_codec_typed().field_of("functions") })),
    ));
    entries.push((
        "filtered",
        entry(record!(FilteredFunction {
            condition: cond(),
            filter: ItemPredicate::codec().field_of("item_filter"),
            on_pass: function_holder_codec().optional_field_of("on_pass"),
            on_fail: function_holder_codec().optional_field_of("on_fail"),
        })),
    ));
    entries.push((
        "copy_state",
        entry(record!(CopyBlockStateRaw {
            condition: cond(),
            block: mc_protocol::components::holder::registry_by_name_codec::<Block>().field_of("block"),
            properties: STRING.list_of().field_of("properties"),
        })
        .xmap(
            |raw| {
                let names: Vec<String> = raw.block.default_state().values().map(|(d, _)| d.name.to_owned()).collect();
                // `Collectors.toSet()` of the properties that exist on the block
                CopyBlockState {
                    condition: raw.condition,
                    block: raw.block,
                    properties: raw.properties.into_iter().filter(|p| names.contains(p)).collect(),
                }
            },
            |f: &CopyBlockState| CopyBlockStateRaw { condition: f.condition.clone(), block: f.block, properties: f.properties.clone() },
        )),
    ));
    entries.push((
        "copy_components",
        entry(record!(CopyComponentsFunction {
            condition: cond(),
            source: ComponentSourceCodec::codec().field_of("source"),
            include: mc_protocol::components::map::type_codec().list_of().optional_field_of("include"),
            exclude: mc_protocol::components::map::type_codec().list_of().optional_field_of("exclude"),
        })),
    ));
    entries.push((
        "copy_custom_data",
        entry(record!(CopyCustomDataFunction {
            condition: cond(),
            source: nbt_provider_codec().field_of("source"),
            operations: copy_operation_codec().list_of().field_of("ops"),
        })),
    ));
    entries.push((
        "fill_player_head",
        entry(record!(FillPlayerHead { condition: cond(), entity_target: EntityTarget::codec().field_of("entity") })),
    ));
    entries.push((
        "set_loot_table",
        entry(record!(SetContainerLootTable {
            condition: cond(),
            loot_table_id: id_only_loot_table_codec().field_of("loot_table_id"),
            seed: LONG.optional_field_of_default("seed", 0),
        })),
    ));
    let _ = (FLOAT.clone(), MERGE_STRATEGIES);
}

#[derive(Clone)]
struct CopyBlockStateRaw {
    condition: Option<ConditionHolder>,
    block: Block,
    properties: Vec<String>,
}

struct ComponentSourceCodec;

impl ComponentSourceCodec {
    fn codec() -> Codec<ComponentSource> {
        static NAMES: [(ComponentSource, &str); 2] = [(ComponentSource::BlockEntity, "block_entity"), (ComponentSource::Tool, "tool")];
        // `LootContextArg` names: every `EntityTarget`/`ItemStackTarget`/`BlockEntityTarget`; only the item/block ones read components
        mc_dfu::string_enum_codec(&NAMES)
    }
}

fn copy_operation_codec() -> Codec<CopyOperation> {
    record!(CopyOperation {
        source_path: nbt_path_codec().field_of("source"),
        target_path: nbt_path_codec().field_of("target"),
        op: mc_dfu::string_enum_codec(MERGE_STRATEGIES).field_of("op"),
    })
    .codec()
}

/// `SetContainerLootTable.ID_ONLY_CODEC`: a `loot_table` registry id (inline definitions are rejected).
fn id_only_loot_table_codec() -> Codec<Identifier> {
    crate::world::level::storage::loot::holder::registry_ref_codec(keys::LOOT_TABLE)
}

#[allow(unused)]
fn _unused(_: &Option<CompoundTag>) {}
