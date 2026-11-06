//! `net.minecraft.util.context.{ContextKey, ContextKeySet, ContextMap}`, `LootParams` and `LootContext`.
//!
//! Vanilla keeps the loot parameters in an untyped `ContextMap`; here every vanilla key is a typed field of
//! [`LootParams`] (entities are borrowed `&dyn Entity`: the entity that is currently running is taken out of the level's
//! storage, so callers pass it explicitly) and [`ContextKey`] names the fields for parameter sets and validation.

use std::collections::HashMap;

use mc_core::ident::Identifier;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_protocol::components::ItemStack;

use super::LootTable;
use super::functions::LootItemFunctionRef;
use super::predicates::ConditionRef;
use crate::registry::RegistryAccess;
use crate::server::level::ServerLevel;
use crate::world::block_entity::BlockEntity;
use crate::world::damagesource::DamageSource;
use crate::world::entity::Entity;
use crate::world::item::slot::SlotProvider;

/// `LootContextParams.*` (`ContextKey.vanilla(name)`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum ContextKey {
    ThisEntity,
    InteractingEntity,
    TargetEntity,
    LastDamagePlayer,
    DamageSource,
    AttackingEntity,
    DirectAttackingEntity,
    Origin,
    BlockState,
    BlockEntity,
    Tool,
    ExplosionRadius,
    EnchantmentLevel,
    EnchantmentActive,
    AdditionalCostComponentAllowed,
    Container,
}

impl ContextKey {
    /// `ContextKey.name().getPath()` (the namespace is always `minecraft`).
    pub fn name(self) -> &'static str {
        match self {
            ContextKey::ThisEntity => "this_entity",
            ContextKey::InteractingEntity => "interacting_entity",
            ContextKey::TargetEntity => "target_entity",
            ContextKey::LastDamagePlayer => "last_damage_player",
            ContextKey::DamageSource => "damage_source",
            ContextKey::AttackingEntity => "attacking_entity",
            ContextKey::DirectAttackingEntity => "direct_attacking_entity",
            ContextKey::Origin => "origin",
            ContextKey::BlockState => "block_state",
            ContextKey::BlockEntity => "block_entity",
            ContextKey::Tool => "tool",
            ContextKey::ExplosionRadius => "explosion_radius",
            ContextKey::EnchantmentLevel => "enchantment_level",
            ContextKey::EnchantmentActive => "enchantment_active",
            ContextKey::AdditionalCostComponentAllowed => "additional_cost_component_allowed",
            ContextKey::Container => "container",
        }
    }
}

impl std::fmt::Display for ContextKey {
    /// `ContextKey.toString`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<parameter minecraft:{}>", self.name())
    }
}

/// `ContextKeySet`: required and optional keys (`allowed = required ∪ optional`).
#[derive(Clone, Copy, Debug)]
pub struct ContextKeySet {
    pub required: &'static [ContextKey],
    pub optional: &'static [ContextKey],
}

impl ContextKeySet {
    pub const fn new(
        required: &'static [ContextKey],
        optional: &'static [ContextKey],
    ) -> ContextKeySet {
        ContextKeySet { required, optional }
    }

    pub fn is_required(&self, key: ContextKey) -> bool {
        self.required.contains(&key)
    }

    /// `allowed().contains(key)`.
    pub fn allows(&self, key: ContextKey) -> bool {
        self.required.contains(&key) || self.optional.contains(&key)
    }

    /// `allowed()` (required first, then optional).
    pub fn allowed(&self) -> impl Iterator<Item = ContextKey> + '_ {
        self.required.iter().chain(self.optional.iter()).copied()
    }
}

impl PartialEq for ContextKeySet {
    /// `ContextKeySet` has identity equality in vanilla and every set is a registered singleton; compare the contents.
    fn eq(&self, other: &Self) -> bool {
        self.required == other.required && self.optional == other.optional
    }
}

impl std::fmt::Display for ContextKeySet {
    /// `ContextKeySet.toString`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self
            .allowed()
            .map(|k| {
                format!(
                    "{}minecraft:{}",
                    if self.is_required(k) { "!" } else { "" },
                    k.name()
                )
            })
            .collect();
        write!(f, "[{}]", parts.join(", "))
    }
}

/// `LootParams.DynamicDrop`.
pub type DynamicDrop<'a> = Box<dyn Fn(&mut dyn FnMut(ItemStack)) + 'a>;

/// `LootParams`: the validated parameters of one loot roll.
pub struct LootParams<'a> {
    level: &'a ServerLevel,
    pub this_entity: Option<&'a dyn Entity>,
    pub interacting_entity: Option<&'a dyn Entity>,
    pub target_entity: Option<&'a dyn Entity>,
    /// `LAST_DAMAGE_PLAYER` (a `Player`).
    pub last_damage_player: Option<&'a dyn Entity>,
    pub attacking_entity: Option<&'a dyn Entity>,
    pub direct_attacking_entity: Option<&'a dyn Entity>,
    pub damage_source: Option<&'a DamageSource>,
    pub origin: Option<Vec3>,
    pub block_state: Option<BlockState>,
    pub block_entity: Option<&'a dyn BlockEntity>,
    /// `TOOL` (`ItemInstance`; loot only ever passes stacks).
    pub tool: Option<ItemStack>,
    pub explosion_radius: Option<f32>,
    pub enchantment_level: Option<i32>,
    pub enchantment_active: Option<bool>,
    /// `ADDITIONAL_COST_COMPONENT_ALLOWED` (`Unit`).
    pub additional_cost_component_allowed: bool,
    pub container: Option<&'a dyn SlotProvider>,
    dynamic_drops: HashMap<Identifier, DynamicDrop<'a>>,
    luck: f32,
}

impl<'a> LootParams<'a> {
    /// `new LootParams.Builder(level)`.
    pub fn builder(level: &'a ServerLevel) -> LootParamsBuilder<'a> {
        LootParamsBuilder {
            params: LootParams::empty(level),
        }
    }

    fn empty(level: &'a ServerLevel) -> LootParams<'a> {
        LootParams {
            level,
            this_entity: None,
            interacting_entity: None,
            target_entity: None,
            last_damage_player: None,
            attacking_entity: None,
            direct_attacking_entity: None,
            damage_source: None,
            origin: None,
            block_state: None,
            block_entity: None,
            tool: None,
            explosion_radius: None,
            enchantment_level: None,
            enchantment_active: None,
            additional_cost_component_allowed: false,
            container: None,
            dynamic_drops: HashMap::new(),
            luck: 0.0,
        }
    }

    /// `getLevel()`.
    pub fn level(&self) -> &'a ServerLevel {
        self.level
    }

    /// `getLuck()`.
    pub fn luck(&self) -> f32 {
        self.luck
    }

    /// `contextMap().has(key)`.
    pub fn has(&self, key: ContextKey) -> bool {
        match key {
            ContextKey::ThisEntity => self.this_entity.is_some(),
            ContextKey::InteractingEntity => self.interacting_entity.is_some(),
            ContextKey::TargetEntity => self.target_entity.is_some(),
            ContextKey::LastDamagePlayer => self.last_damage_player.is_some(),
            ContextKey::DamageSource => self.damage_source.is_some(),
            ContextKey::AttackingEntity => self.attacking_entity.is_some(),
            ContextKey::DirectAttackingEntity => self.direct_attacking_entity.is_some(),
            ContextKey::Origin => self.origin.is_some(),
            ContextKey::BlockState => self.block_state.is_some(),
            ContextKey::BlockEntity => self.block_entity.is_some(),
            ContextKey::Tool => self.tool.is_some(),
            ContextKey::ExplosionRadius => self.explosion_radius.is_some(),
            ContextKey::EnchantmentLevel => self.enchantment_level.is_some(),
            ContextKey::EnchantmentActive => self.enchantment_active.is_some(),
            ContextKey::AdditionalCostComponentAllowed => self.additional_cost_component_allowed,
            ContextKey::Container => self.container.is_some(),
        }
    }

    /// The entity stored under an entity key (`EntityTarget.contextParam()`).
    pub fn entity(&self, key: ContextKey) -> Option<&'a dyn Entity> {
        match key {
            ContextKey::ThisEntity => self.this_entity,
            ContextKey::InteractingEntity => self.interacting_entity,
            ContextKey::TargetEntity => self.target_entity,
            ContextKey::LastDamagePlayer => self.last_damage_player,
            ContextKey::AttackingEntity => self.attacking_entity,
            ContextKey::DirectAttackingEntity => self.direct_attacking_entity,
            _ => None,
        }
    }

    fn keys(&self) -> impl Iterator<Item = ContextKey> + '_ {
        ALL_KEYS.iter().copied().filter(|k| self.has(*k))
    }
}

const ALL_KEYS: [ContextKey; 16] = [
    ContextKey::ThisEntity,
    ContextKey::InteractingEntity,
    ContextKey::TargetEntity,
    ContextKey::LastDamagePlayer,
    ContextKey::DamageSource,
    ContextKey::AttackingEntity,
    ContextKey::DirectAttackingEntity,
    ContextKey::Origin,
    ContextKey::BlockState,
    ContextKey::BlockEntity,
    ContextKey::Tool,
    ContextKey::ExplosionRadius,
    ContextKey::EnchantmentLevel,
    ContextKey::EnchantmentActive,
    ContextKey::AdditionalCostComponentAllowed,
    ContextKey::Container,
];

/// `LootParams.Builder`.
pub struct LootParamsBuilder<'a> {
    params: LootParams<'a>,
}

impl<'a> LootParamsBuilder<'a> {
    pub fn level(&self) -> &'a ServerLevel {
        self.params.level
    }
    pub fn this_entity(mut self, v: &'a dyn Entity) -> Self {
        self.params.this_entity = Some(v);
        self
    }
    pub fn optional_this_entity(mut self, v: Option<&'a dyn Entity>) -> Self {
        self.params.this_entity = v;
        self
    }
    pub fn interacting_entity(mut self, v: Option<&'a dyn Entity>) -> Self {
        self.params.interacting_entity = v;
        self
    }
    pub fn target_entity(mut self, v: &'a dyn Entity) -> Self {
        self.params.target_entity = Some(v);
        self
    }
    pub fn last_damage_player(mut self, v: Option<&'a dyn Entity>) -> Self {
        self.params.last_damage_player = v;
        self
    }
    pub fn attacking_entity(mut self, v: Option<&'a dyn Entity>) -> Self {
        self.params.attacking_entity = v;
        self
    }
    pub fn direct_attacking_entity(mut self, v: Option<&'a dyn Entity>) -> Self {
        self.params.direct_attacking_entity = v;
        self
    }
    pub fn damage_source(mut self, v: &'a DamageSource) -> Self {
        self.params.damage_source = Some(v);
        self
    }
    pub fn origin(mut self, v: Vec3) -> Self {
        self.params.origin = Some(v);
        self
    }
    pub fn block_state(mut self, v: BlockState) -> Self {
        self.params.block_state = Some(v);
        self
    }
    pub fn block_entity(mut self, v: Option<&'a dyn BlockEntity>) -> Self {
        self.params.block_entity = v;
        self
    }
    pub fn tool(mut self, v: &ItemStack) -> Self {
        self.params.tool = Some(v.clone());
        self
    }
    pub fn explosion_radius(mut self, v: f32) -> Self {
        self.params.explosion_radius = Some(v);
        self
    }
    pub fn optional_explosion_radius(mut self, v: Option<f32>) -> Self {
        self.params.explosion_radius = v;
        self
    }
    pub fn enchantment_level(mut self, v: i32) -> Self {
        self.params.enchantment_level = Some(v);
        self
    }
    pub fn enchantment_active(mut self, v: bool) -> Self {
        self.params.enchantment_active = Some(v);
        self
    }
    pub fn additional_cost_component_allowed(mut self) -> Self {
        self.params.additional_cost_component_allowed = true;
        self
    }
    pub fn container(mut self, v: &'a dyn SlotProvider) -> Self {
        self.params.container = Some(v);
        self
    }
    /// `withDynamicDrop(location, dynamicDrop)`; a duplicate is an `IllegalStateException`.
    pub fn dynamic_drop(mut self, location: Identifier, drop: DynamicDrop<'a>) -> Self {
        if self.params.dynamic_drops.insert(location, drop).is_some() {
            panic!("Duplicated dynamic drop");
        }
        self
    }
    /// `withLuck(luck)`.
    pub fn luck(mut self, luck: f32) -> Self {
        self.params.luck = luck;
        self
    }
    /// `getParameter(param)` presence (used by callers that only read what they put in).
    pub fn has(&self, key: ContextKey) -> bool {
        self.params.has(key)
    }

    /// `create(contextKeySet)`: `ContextMap.Builder.buildAndValidate` (panics like the `IllegalArgumentException`).
    pub fn create(self, set: &ContextKeySet) -> LootParams<'a> {
        let not_allowed: Vec<String> = self
            .params
            .keys()
            .filter(|k| !set.allows(*k))
            .map(|k| k.to_string())
            .collect();
        if !not_allowed.is_empty() {
            panic!(
                "Parameters not allowed in this parameter set: [{}]",
                not_allowed.join(", ")
            );
        }
        let missing: Vec<String> = set
            .required
            .iter()
            .filter(|k| !self.params.has(**k))
            .map(|k| k.to_string())
            .collect();
        if !missing.is_empty() {
            panic!("Missing required parameters: [{}]", missing.join(", "));
        }
        self.params
    }
}

/// `LootDataType` element kinds that can be on the visited stack (`LootContext.VisitedEntry`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VisitedEntry {
    Table(*const LootTable),
    Predicate(*const ()),
    Modifier(*const ()),
    SlotSource(*const ()),
}

// SAFETY: the pointers are only compared for identity while the referenced registry values (kept alive by the context's
// registry access) exist; they are never dereferenced.
unsafe impl Send for VisitedEntry {}

impl VisitedEntry {
    pub fn table(table: &LootTable) -> VisitedEntry {
        VisitedEntry::Table(table as *const LootTable)
    }
    pub fn predicate(c: &ConditionRef) -> VisitedEntry {
        VisitedEntry::Predicate(std::sync::Arc::as_ptr(c) as *const ())
    }
    pub fn modifier(f: &LootItemFunctionRef) -> VisitedEntry {
        VisitedEntry::Modifier(std::sync::Arc::as_ptr(f) as *const ())
    }
    pub fn slot_source(s: &crate::world::item::slot::SlotSourceRef) -> VisitedEntry {
        VisitedEntry::SlotSource(std::sync::Arc::as_ptr(s) as *const ())
    }
}

/// `LootContext`.
pub struct LootContext<'a> {
    params: &'a LootParams<'a>,
    random: Box<dyn RandomSource>,
    resolver: &'a RegistryAccess,
    visited: Vec<VisitedEntry>,
}

impl<'a> LootContext<'a> {
    /// `new LootContext.Builder(params)`.
    pub fn builder(params: &'a LootParams<'a>) -> LootContextBuilder<'a> {
        LootContextBuilder {
            params,
            random: None,
        }
    }

    pub fn params(&self) -> &'a LootParams<'a> {
        self.params
    }

    /// `hasParameter(key)`.
    pub fn has_parameter(&self, key: ContextKey) -> bool {
        self.params.has(key)
    }

    /// `getLevel()`.
    pub fn get_level(&self) -> &'a ServerLevel {
        self.params.level
    }

    /// `getResolver()` (`server.reloadableRegistries().lookup()`: the level's registries).
    pub fn get_resolver(&self) -> &'a RegistryAccess {
        self.resolver
    }

    /// `getRandom()`.
    pub fn get_random(&mut self) -> &mut dyn RandomSource {
        &mut *self.random
    }

    /// `getLuck()`.
    pub fn get_luck(&self) -> f32 {
        self.params.luck
    }

    /// `addDynamicDrops(location, output)`.
    pub fn add_dynamic_drops(&self, location: &Identifier, output: &mut dyn FnMut(ItemStack)) {
        if let Some(drop) = self.params.dynamic_drops.get(location) {
            drop(output);
        }
    }

    /// `hasVisitedElement(element)`.
    pub fn has_visited_element(&self, element: &VisitedEntry) -> bool {
        self.visited.contains(element)
    }

    /// `pushVisitedElement(element)`: `false` when it is already on the stack.
    pub fn push_visited_element(&mut self, element: VisitedEntry) -> bool {
        if self.visited.contains(&element) {
            false
        } else {
            self.visited.push(element);
            true
        }
    }

    /// `popVisitedElement(element)`.
    pub fn pop_visited_element(&mut self, element: &VisitedEntry) {
        if let Some(i) = self.visited.iter().position(|e| e == element) {
            self.visited.remove(i);
        }
    }
}

/// `LootContext.Builder`.
pub struct LootContextBuilder<'a> {
    params: &'a LootParams<'a>,
    random: Option<Box<dyn RandomSource>>,
}

impl<'a> LootContextBuilder<'a> {
    /// `withOptionalRandomSeed(seed)`: `0` keeps the default source.
    pub fn with_optional_random_seed(mut self, seed: i64) -> Self {
        if seed != 0 {
            self.random = Some(Box::new(mc_core::random::LegacyRandomSource::new(seed)));
        }
        self
    }

    /// `withOptionalRandomSource(randomSource)`.
    pub fn with_optional_random_source(mut self, random: Box<dyn RandomSource>) -> Self {
        self.random = Some(random);
        self
    }

    pub fn get_level(&self) -> &'a ServerLevel {
        self.params.level
    }

    /// `create(randomSequenceKey)`: the explicit source, else the table's random sequence, else the level's random.
    pub fn create(self, random_sequence_key: Option<&Identifier>) -> LootContext<'a> {
        let level = self.params.level;
        let random = match self.random {
            Some(r) => r,
            None => match random_sequence_key {
                Some(key) => super::server_access::random_sequence(level, key),
                None => super::server_access::level_random(level),
            },
        };
        LootContext {
            params: self.params,
            random,
            resolver: &level.registries,
            visited: Vec::new(),
        }
    }
}

/// `LootContext.EntityTarget`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityTarget {
    This,
    Attacker,
    DirectAttacker,
    AttackingPlayer,
    TargetEntity,
    InteractingEntity,
}

impl EntityTarget {
    pub const ALL: [EntityTarget; 6] = [
        EntityTarget::This,
        EntityTarget::Attacker,
        EntityTarget::DirectAttacker,
        EntityTarget::AttackingPlayer,
        EntityTarget::TargetEntity,
        EntityTarget::InteractingEntity,
    ];

    /// `getSerializedName()`.
    pub fn name(self) -> &'static str {
        match self {
            EntityTarget::This => "this",
            EntityTarget::Attacker => "attacker",
            EntityTarget::DirectAttacker => "direct_attacker",
            EntityTarget::AttackingPlayer => "attacking_player",
            EntityTarget::TargetEntity => "target_entity",
            EntityTarget::InteractingEntity => "interacting_entity",
        }
    }

    /// `contextParam()`.
    pub fn context_param(self) -> ContextKey {
        match self {
            EntityTarget::This => ContextKey::ThisEntity,
            EntityTarget::Attacker => ContextKey::AttackingEntity,
            EntityTarget::DirectAttacker => ContextKey::DirectAttackingEntity,
            EntityTarget::AttackingPlayer => ContextKey::LastDamagePlayer,
            EntityTarget::TargetEntity => ContextKey::TargetEntity,
            EntityTarget::InteractingEntity => ContextKey::InteractingEntity,
        }
    }

    /// `get(context)`: `context.getOptional(contextParam())`.
    pub fn get<'a>(self, context: &LootContext<'a>) -> Option<&'a dyn Entity> {
        context.params.entity(self.context_param())
    }

    /// `EntityTarget.CODEC`.
    pub fn codec() -> mc_dfu::Codec<EntityTarget> {
        static NAMES: [(EntityTarget, &str); 6] = [
            (EntityTarget::This, "this"),
            (EntityTarget::Attacker, "attacker"),
            (EntityTarget::DirectAttacker, "direct_attacker"),
            (EntityTarget::AttackingPlayer, "attacking_player"),
            (EntityTarget::TargetEntity, "target_entity"),
            (EntityTarget::InteractingEntity, "interacting_entity"),
        ];
        mc_dfu::string_enum_codec(&NAMES)
    }
}
