//! `GameRuleMap` (`data/minecraft/game_rules.dat`).

use mc_core::ident::Identifier;
use mc_data::misc::GAME_RULE_DATA;
use mc_dfu::extra::{IDENTIFIER, dispatched_map};
use mc_dfu::{Codec, DataResult};
use mc_nbt::Tag;

use super::{AnyGameRule, GameRule, GameRuleValue, GameRuleValueType};
use crate::impl_saved_data;
use crate::util::codec::DecodeResult;
use crate::world::level::saveddata::{SavedData, SavedDataType, decode_codec};

/// `GameRuleMap`: rule -> value for a subset of the registry. Iteration is registry order (vanilla's
/// `Reference2ObjectOpenHashMap` order is identity-hash dependent and unobservable).
#[derive(Debug, Clone)]
pub struct GameRuleMap {
    dirty: bool,
    values: [Option<GameRuleValue>; GAME_RULE_DATA.len()],
}

impl_saved_data!(GameRuleMap);

impl PartialEq for GameRuleMap {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl Default for GameRuleMap {
    fn default() -> Self {
        GameRuleMap::of_empty()
    }
}

impl GameRuleMap {
    /// `of()`.
    pub fn of_empty() -> GameRuleMap {
        GameRuleMap {
            dirty: false,
            values: [None; GAME_RULE_DATA.len()],
        }
    }

    /// `of(Stream<GameRule<?>>)`: every given rule at its default.
    pub fn of(rules: impl IntoIterator<Item = AnyGameRule>) -> GameRuleMap {
        let mut map = GameRuleMap::of_empty();
        for rule in rules {
            map.values[rule.registry_id()] = Some(rule.default_value());
        }
        map
    }

    /// `copyOf`: the copy is not dirty.
    pub fn copy_of(other: &GameRuleMap) -> GameRuleMap {
        GameRuleMap {
            dirty: false,
            values: other.values,
        }
    }

    pub fn has(&self, rule: AnyGameRule) -> bool {
        self.values[rule.registry_id()].is_some()
    }

    /// `get`: `None` when the rule is not in the map.
    pub fn get<T: GameRuleValueType>(&self, rule: GameRule<T>) -> Option<T> {
        self.get_value(rule.erase()).and_then(T::from_value)
    }

    pub fn get_value(&self, rule: AnyGameRule) -> Option<GameRuleValue> {
        self.values[rule.registry_id()]
    }

    pub fn set<T: GameRuleValueType>(&mut self, rule: GameRule<T>, value: T) {
        self.set_value(rule.erase(), value.into_value());
    }

    pub fn set_value(&mut self, rule: AnyGameRule, value: GameRuleValue) {
        debug_assert_eq!(
            rule.game_rule_type(),
            match value {
                GameRuleValue::Bool(_) => super::GameRuleType::Bool,
                GameRuleValue::Int(_) => super::GameRuleType::Int,
            }
        );
        self.set_dirty();
        self.values[rule.registry_id()] = Some(value);
    }

    pub fn reset(&mut self, rule: AnyGameRule) {
        self.set_value(rule, rule.default_value());
    }

    pub fn remove(&mut self, rule: AnyGameRule) -> Option<GameRuleValue> {
        self.set_dirty();
        self.values[rule.registry_id()].take()
    }

    /// `keySet()`.
    pub fn key_set(&self) -> impl Iterator<Item = AnyGameRule> + '_ {
        AnyGameRule::all().filter(|r| self.has(*r))
    }

    pub fn size(&self) -> usize {
        self.values.iter().flatten().count()
    }

    /// `withOther`: a copy overridden by every value of `other`.
    pub fn with_other(&self, other: &GameRuleMap) -> GameRuleMap {
        let mut result = GameRuleMap::copy_of(self);
        result.set_from_if(other, |_| true);
        result
    }

    pub fn set_from_if(&mut self, other: &GameRuleMap, predicate: impl Fn(AnyGameRule) -> bool) {
        for rule in other.key_set() {
            if predicate(rule) {
                self.set_value(rule, other.get_value(rule).expect("key of other"));
            }
        }
    }

    /// `CODEC` (`Codec.dispatchedMap(GAME_RULE.byNameCodec(), GameRule::valueCodec).xmap(ofTrusted, map)`): entries with
    /// an unknown rule, a wrong type or an out-of-range value are dropped and reported as a partial result.
    pub fn codec() -> Codec<GameRuleMap> {
        let rule = IDENTIFIER.flat_xmap(
            |id| match AnyGameRule::by_identifier(&id) {
                Some(rule) => DataResult::success(rule),
                None => DataResult::error(move || {
                    format!(
                        "Unknown registry key in ResourceKey[minecraft:root / minecraft:game_rule]: {id}"
                    )
                }),
            },
            |rule: &AnyGameRule| DataResult::success(rule.get_identifier()),
        );
        dispatched_map(&rule, |rule: &AnyGameRule| {
            DataResult::success(rule.value_codec())
        })
        .xmap(
            |entries| {
                let mut map = GameRuleMap::of_empty();
                for (rule, value) in entries {
                    map.values[rule.registry_id()] = Some(value);
                }
                map
            },
            |map: &GameRuleMap| {
                map.key_set()
                    .map(|rule| (rule, map.get_value(rule).expect("key")))
                    .collect()
            },
        )
    }

    #[expect(
        clippy::result_large_err,
        reason = "DecodeResult<T> carries the partial value by value (DataResult.error(msg, partial))"
    )]
    pub fn decode(tag: &Tag) -> DecodeResult<GameRuleMap> {
        decode_codec(&GameRuleMap::codec(), tag)
    }

    /// `GameRuleMap.TYPE`.
    pub fn type_() -> SavedDataType<GameRuleMap> {
        SavedDataType::from_codec(
            Identifier::parse("minecraft:game_rules").expect("static id"),
            GameRuleMap::of_empty,
            GameRuleMap::codec(),
            mc_datafix::DataFixTypes::SAVED_DATA_GAME_RULES,
        )
    }
}

/// `GameRuleMap.Builder`.
#[derive(Default)]
pub struct GameRuleMapBuilder {
    map: GameRuleMap,
}

impl GameRuleMapBuilder {
    pub fn set<T: GameRuleValueType>(mut self, rule: GameRule<T>, value: T) -> Self {
        self.map.values[rule.erase().registry_id()] = Some(value.into_value());
        self
    }

    pub fn build(self) -> GameRuleMap {
        self.map
    }
}
