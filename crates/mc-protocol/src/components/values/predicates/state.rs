//! `StatePropertiesPredicate` and `NbtPredicate`.

use crate::codec::{Decode, DecodeResult, Encode};
use crate::components::common::lenient_compound_codec;
use mc_dfu::{Codec, DataResult, Either, STRING};
use mc_nbt::{CompoundTag, Tag};
use std::sync::LazyLock;

/// `StatePropertiesPredicate.ValueMatcher`: an exact value or a `{min?, max?}` range (property values are strings).
#[derive(Clone, Debug, PartialEq)]
pub enum ValueMatcher {
    Exact(String),
    Ranged { min: Option<String>, max: Option<String> },
}

/// `StatePropertiesPredicate.PropertyMatcher`.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyMatcher {
    pub name: String,
    pub value_matcher: ValueMatcher,
}

#[derive(Clone, Debug, PartialEq)]
struct Ranged {
    min: Option<String>,
    max: Option<String>,
}

impl ValueMatcher {
    fn codec() -> Codec<ValueMatcher> {
        static C: LazyLock<Codec<ValueMatcher>> = LazyLock::new(|| {
            let ranged =
                mc_dfu::record!(Ranged { min: STRING.optional_field_of("min"), max: STRING.optional_field_of("max") }).codec();
            STRING.either(&ranged).xmap(
                |e| match e {
                    Either::Left(v) => ValueMatcher::Exact(v),
                    Either::Right(Ranged { min, max }) => ValueMatcher::Ranged { min, max },
                },
                |m: &ValueMatcher| match m {
                    ValueMatcher::Exact(v) => Either::Left(v.clone()),
                    ValueMatcher::Ranged { min, max } => Either::Right(Ranged { min: min.clone(), max: max.clone() }),
                },
            )
        });
        C.clone()
    }
}

impl Encode for ValueMatcher {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            ValueMatcher::Exact(v) => {
                out.push(1);
                v.encode(out);
            }
            ValueMatcher::Ranged { min, max } => {
                out.push(0);
                min.encode(out);
                max.encode(out);
            }
        }
    }
}
impl Decode for ValueMatcher {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        if bool::decode(input)? {
            String::decode(input).map(ValueMatcher::Exact)
        } else {
            Ok(ValueMatcher::Ranged { min: Option::<String>::decode(input)?, max: Option::<String>::decode(input)? })
        }
    }
}
impl Encode for PropertyMatcher {
    fn encode(&self, out: &mut Vec<u8>) {
        self.name.encode(out);
        self.value_matcher.encode(out);
    }
}
impl Decode for PropertyMatcher {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(PropertyMatcher { name: String::decode(input)?, value_matcher: ValueMatcher::decode(input)? })
    }
}

/// `StatePropertiesPredicate`: property name -> matcher.
#[derive(Clone, Debug, PartialEq)]
pub struct StatePropertiesPredicate {
    pub properties: Vec<PropertyMatcher>,
}

impl StatePropertiesPredicate {
    /// `StatePropertiesPredicate.CODEC`: a `{name: value | {min?, max?}}` map. Encoding goes through a
    /// `Collectors.toMap` (a `HashMap`), so the written key order is Java's hash order and duplicate names fail.
    pub fn codec() -> Codec<StatePropertiesPredicate> {
        static C: LazyLock<Codec<StatePropertiesPredicate>> = LazyLock::new(|| {
            STRING.unbounded_map(&ValueMatcher::codec()).flat_xmap(
                |map| {
                    DataResult::success(StatePropertiesPredicate {
                        properties: map.into_iter().map(|(name, value_matcher)| PropertyMatcher { name, value_matcher }).collect(),
                    })
                },
                |p: &StatePropertiesPredicate| {
                    let mut seen = std::collections::HashSet::new();
                    if let Some(dup) = p.properties.iter().find(|m| !seen.insert(m.name.as_str())) {
                        let m = format!("Duplicate key {}", dup.name);
                        return DataResult::error(move || m.clone());
                    }
                    let entries = p.properties.iter().map(|m| (m.name.clone(), m.value_matcher.clone())).collect();
                    DataResult::success(mc_dfu::extra::java_hash_map_order(entries))
                },
            )
        });
        C.clone()
    }
}

impl Encode for StatePropertiesPredicate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.properties.encode(out)
    }
}
impl Decode for StatePropertiesPredicate {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Vec::<PropertyMatcher>::decode(input).map(|properties| StatePropertiesPredicate { properties })
    }
}

/// `NbtPredicate`: a compound that must be (partially) contained in the tested tag.
#[derive(Clone, Debug, PartialEq)]
pub struct NbtPredicate {
    pub tag: CompoundTag,
}

impl NbtPredicate {
    /// `NbtPredicate.CODEC` (`TagParser.LENIENT_CODEC`): an SNBT string or a compound.
    pub fn codec() -> Codec<NbtPredicate> {
        static C: LazyLock<Codec<NbtPredicate>> =
            LazyLock::new(|| lenient_compound_codec().xmap(|tag| NbtPredicate { tag }, |p: &NbtPredicate| p.tag.clone()));
        C.clone()
    }

    /// `matches(Tag)`: `NbtUtils.compareNbt(tag, other, true)`.
    pub fn matches_tag(&self, other: Option<&Tag>) -> bool {
        other.is_some() && mc_nbt::nbt_utils::compare_nbt(Some(&Tag::Compound(self.tag.clone())), other, true)
    }
}

impl Encode for NbtPredicate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.tag.encode(out)
    }
}
impl Decode for NbtPredicate {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        CompoundTag::decode(input).map(|tag| NbtPredicate { tag })
    }
}
