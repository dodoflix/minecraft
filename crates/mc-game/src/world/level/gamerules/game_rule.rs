//! `GameRule<T>`, `GameRuleCategory`, `GameRuleType`, `GameRuleTypeVisitor`.

use std::marker::PhantomData;

use mc_brigadier::arguments::{ArgumentType, BoolArgumentType, IntegerArgumentType};
use mc_brigadier::string_reader::StringReader;
use mc_core::ident::Identifier;
use mc_data::RegistryId;
use mc_data::misc::GAME_RULE_DATA;
use mc_nbt::Tag;
use mc_text::Component;
use mc_world::level_storage::FeatureFlagSet;

use crate::util::codec::{self, DataResult, Partial};
use crate::util::util::make_description_id;

pub use mc_data::misc::GameRuleValue;

/// `GameRuleType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameRuleType {
    Int,
    Bool,
}

impl GameRuleType {
    /// `getSerializedName`.
    pub fn get_serialized_name(self) -> &'static str {
        match self {
            GameRuleType::Int => "integer",
            GameRuleType::Bool => "boolean",
        }
    }
}

/// `GameRuleCategory` (a registered id; `SORT_ORDER` is the registration order below).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GameRuleCategory {
    id: Identifier,
}

impl GameRuleCategory {
    /// Registration order (`SORT_ORDER`): player, mobs, spawning, drops, updates, chat, misc.
    pub const SORT_ORDER: [&'static str; 7] =
        ["minecraft:player", "minecraft:mobs", "minecraft:spawning", "minecraft:drops", "minecraft:updates", "minecraft:chat", "minecraft:misc"];

    pub fn of(id: &str) -> GameRuleCategory {
        GameRuleCategory { id: Identifier::parse(id).expect("category ids are static") }
    }

    pub fn player() -> GameRuleCategory {
        Self::of("minecraft:player")
    }
    pub fn mobs() -> GameRuleCategory {
        Self::of("minecraft:mobs")
    }
    pub fn spawning() -> GameRuleCategory {
        Self::of("minecraft:spawning")
    }
    pub fn drops() -> GameRuleCategory {
        Self::of("minecraft:drops")
    }
    pub fn updates() -> GameRuleCategory {
        Self::of("minecraft:updates")
    }
    pub fn chat() -> GameRuleCategory {
        Self::of("minecraft:chat")
    }
    pub fn misc() -> GameRuleCategory {
        Self::of("minecraft:misc")
    }

    pub fn id(&self) -> &Identifier {
        &self.id
    }

    /// `getDescriptionId`.
    pub fn get_description_id(&self) -> &Identifier {
        &self.id
    }

    /// `label()`: `gamerule.category.<namespace>.<path>` translatable.
    pub fn label(&self) -> Component {
        Component::translatable(&self.id.to_language_key_prefixed("gamerule.category"), vec![])
    }

    /// Position in `SORT_ORDER` (the order `/gamerule` listings and the UI use).
    pub fn sort_index(&self) -> usize {
        Self::SORT_ORDER.iter().position(|c| *c == self.id.to_string()).unwrap_or(usize::MAX)
    }
}

/// Rust types a rule value can have (vanilla: `Boolean`, `Integer`).
pub trait GameRuleValueType: Copy + PartialEq + std::fmt::Debug + Send + Sync + 'static {
    const TYPE: GameRuleType;
    fn from_value(value: GameRuleValue) -> Option<Self>;
    fn into_value(self) -> GameRuleValue;
    /// `ToIntFunction<T> commandResultFunction` (`/gamerule` query result).
    fn command_result(self) -> i32;
    /// `value.toString()`.
    fn serialize(self) -> String;
}

impl GameRuleValueType for bool {
    const TYPE: GameRuleType = GameRuleType::Bool;
    fn from_value(value: GameRuleValue) -> Option<bool> {
        match value {
            GameRuleValue::Bool(b) => Some(b),
            GameRuleValue::Int(_) => None,
        }
    }
    fn into_value(self) -> GameRuleValue {
        GameRuleValue::Bool(self)
    }
    fn command_result(self) -> i32 {
        i32::from(self)
    }
    fn serialize(self) -> String {
        self.to_string()
    }
}

impl GameRuleValueType for i32 {
    const TYPE: GameRuleType = GameRuleType::Int;
    fn from_value(value: GameRuleValue) -> Option<i32> {
        match value {
            GameRuleValue::Int(i) => Some(i),
            GameRuleValue::Bool(_) => None,
        }
    }
    fn into_value(self) -> GameRuleValue {
        GameRuleValue::Int(self)
    }
    fn command_result(self) -> i32 {
        self
    }
    fn serialize(self) -> String {
        self.to_string()
    }
}

/// `GameRuleTypeVisitor`.
pub trait GameRuleTypeVisitor {
    fn visit(&mut self, _rule: AnyGameRule) {}
    fn visit_boolean(&mut self, _rule: GameRule<bool>) {}
    fn visit_integer(&mut self, _rule: GameRule<i32>) {}
}

/// Type-erased rule handle (vanilla `GameRule<?>`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnyGameRule(mc_data::GameRule);

impl std::fmt::Debug for AnyGameRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id())
    }
}

impl std::fmt::Display for AnyGameRule {
    /// `GameRule.toString` = `id()`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.id())
    }
}

impl AnyGameRule {
    pub const fn new(rule: mc_data::GameRule) -> AnyGameRule {
        AnyGameRule(rule)
    }

    /// Every rule in registry order.
    pub fn all() -> impl Iterator<Item = AnyGameRule> {
        (0..GAME_RULE_DATA.len() as u16).map(|i| AnyGameRule(mc_data::GameRule::from_id(i as u32)))
    }

    pub fn by_identifier(id: &Identifier) -> Option<AnyGameRule> {
        mc_data::GameRule::by_name(&id.to_string()).map(AnyGameRule)
    }

    pub fn registry_id(self) -> usize {
        self.0.id() as usize
    }

    fn data(self) -> &'static mc_data::misc::GameRuleData {
        &GAME_RULE_DATA[self.registry_id()]
    }

    /// `getIdentifier`.
    pub fn get_identifier(self) -> Identifier {
        self.0.identifier()
    }

    /// `id()`: the short string (`random_tick_speed` for `minecraft:` rules).
    pub fn id(self) -> String {
        self.get_identifier().to_short_string()
    }

    /// `getDescriptionId`.
    pub fn get_description_id(self) -> String {
        make_description_id("gamerule", Some(&self.get_identifier()))
    }

    pub fn category(self) -> GameRuleCategory {
        GameRuleCategory::of(self.data().category)
    }

    pub fn game_rule_type(self) -> GameRuleType {
        match self.data().default {
            GameRuleValue::Bool(_) => GameRuleType::Bool,
            GameRuleValue::Int(_) => GameRuleType::Int,
        }
    }

    pub fn default_value(self) -> GameRuleValue {
        self.data().default
    }

    /// `requiredFeatures()`.
    pub fn required_features(self) -> FeatureFlagSet {
        if self.0 == mc_data::GameRule::MAX_MINECART_SPEED { FeatureFlagSet::MINECART_IMPROVEMENTS } else { FeatureFlagSet::EMPTY }
    }

    /// `isEnabled(enabledFeatures)` (`FeatureElement`).
    pub fn is_enabled(self, enabled_features: FeatureFlagSet) -> bool {
        self.required_features().is_subset_of(enabled_features)
    }

    /// `IntegerArgumentType` bounds (`i32::MIN/MAX` for unbounded sides).
    fn int_range(self) -> (i32, i32) {
        (self.data().min.unwrap_or(i32::MIN), self.data().max.unwrap_or(i32::MAX))
    }

    /// `argument()`: the command argument type (`bool()` / `integer(min, max)`).
    pub fn argument<S: 'static>(self) -> Box<dyn ArgumentType<S>> {
        match self.game_rule_type() {
            GameRuleType::Bool => Box::new(BoolArgumentType::bool()),
            GameRuleType::Int => {
                let (min, max) = self.int_range();
                Box::new(IntegerArgumentType::integer_range(min, max))
            }
        }
    }

    /// `serialize(value)`.
    pub fn serialize(self, value: GameRuleValue) -> String {
        match value {
            GameRuleValue::Bool(b) => b.serialize(),
            GameRuleValue::Int(i) => i.serialize(),
        }
    }

    /// `deserialize(String)` through the rule's command argument: parse failures are `Failed to deserialize`,
    /// trailing input is an error that still carries the parsed value (`DataResult.error(msg, result)`).
    pub fn deserialize(self, value: &str) -> Result<GameRuleValue, Partial<GameRuleValue>> {
        let mut reader = StringReader::new(value);
        let parsed = ArgumentType::<()>::parse(&*self.argument::<()>(), &mut reader)
            .ok()
            .and_then(|v| match self.game_rule_type() {
                GameRuleType::Bool => v.downcast_ref::<bool>().copied().map(GameRuleValue::Bool),
                GameRuleType::Int => v.downcast_ref::<i32>().copied().map(GameRuleValue::Int),
            });
        match parsed {
            None => Err(Partial { value: None, error: "Failed to deserialize".to_string() }),
            Some(v) if reader.can_read() => Err(Partial { value: Some(v), error: "Failed to deserialize; trailing characters".to_string() }),
            Some(v) => Ok(v),
        }
    }

    /// `getCommandResult(value)`.
    pub fn get_command_result(self, value: GameRuleValue) -> i32 {
        match value {
            GameRuleValue::Bool(b) => b.command_result(),
            GameRuleValue::Int(i) => i.command_result(),
        }
    }

    /// `valueCodec().decode` on `NbtOps` (`Codec.BOOL`, `Codec.intRange(min, max)`).
    pub fn decode_value(self, tag: &Tag) -> DataResult<GameRuleValue> {
        match self.game_rule_type() {
            GameRuleType::Bool => codec::boolean(tag).map(GameRuleValue::Bool),
            GameRuleType::Int => {
                let v = codec::int(tag)?;
                let (min, max) = self.int_range();
                if (min..=max).contains(&v) {
                    Ok(GameRuleValue::Int(v))
                } else {
                    Err(format!("Value {v} outside of range [{min}:{max}]"))
                }
            }
        }
    }

    /// `valueCodec().encode`.
    pub fn encode_value(self, value: GameRuleValue) -> Tag {
        match value {
            GameRuleValue::Bool(b) => Tag::from_bool(b),
            GameRuleValue::Int(i) => Tag::Int(i),
        }
    }

    /// `callVisitor`.
    pub fn call_visitor(self, visitor: &mut dyn GameRuleTypeVisitor) {
        match self.game_rule_type() {
            GameRuleType::Bool => visitor.visit_boolean(GameRule::new(self.0)),
            GameRuleType::Int => visitor.visit_integer(GameRule::new(self.0)),
        }
    }
}

/// `GameRule<T>`: a typed handle (`GameRules.PVP` ...).
#[derive(PartialEq, Eq, Hash)]
pub struct GameRule<T> {
    rule: mc_data::GameRule,
    _value: PhantomData<fn() -> T>,
}

impl<T> Clone for GameRule<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for GameRule<T> {}

impl<T> std::fmt::Debug for GameRule<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GameRule({})", self.erase())
    }
}

impl<T> GameRule<T> {
    pub const fn new(rule: mc_data::GameRule) -> GameRule<T> {
        GameRule { rule, _value: PhantomData }
    }

    pub const fn erase(self) -> AnyGameRule {
        AnyGameRule(self.rule)
    }
}

impl<T: GameRuleValueType> GameRule<T> {
    /// `defaultValue()`.
    pub fn default_value(self) -> T {
        T::from_value(self.erase().default_value()).expect("typed handle matches the registered rule type")
    }

    pub fn id(self) -> String {
        self.erase().id()
    }

    pub fn get_identifier(self) -> Identifier {
        self.erase().get_identifier()
    }

    pub fn get_description_id(self) -> String {
        self.erase().get_description_id()
    }

    pub fn category(self) -> GameRuleCategory {
        self.erase().category()
    }

    pub fn game_rule_type(self) -> GameRuleType {
        T::TYPE
    }

    pub fn serialize(self, value: T) -> String {
        value.serialize()
    }

    pub fn deserialize(self, value: &str) -> Result<T, Partial<T>> {
        let conv = |v: GameRuleValue| T::from_value(v).expect("typed handle matches the registered rule type");
        self.erase().deserialize(value).map(conv).map_err(|p| Partial { value: p.value.map(conv), error: p.error })
    }

    pub fn get_command_result(self, value: T) -> i32 {
        value.command_result()
    }

    pub fn required_features(self) -> FeatureFlagSet {
        self.erase().required_features()
    }

    pub fn is_enabled(self, enabled_features: FeatureFlagSet) -> bool {
        self.erase().is_enabled(enabled_features)
    }

    pub fn argument<S: 'static>(self) -> Box<dyn ArgumentType<S>> {
        self.erase().argument()
    }
}
