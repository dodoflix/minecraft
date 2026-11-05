//! `DataComponentPredicate` (the `data_component_predicate_type` registry and `AnyValue`) and
//! `DataComponentMatchers` (`exact` components + `partial` predicates).

use crate::codec::EncodeResult;
use super::component_predicates::*;
use crate::codec::{read_count, Decode, DecodeError, DecodeResult, Encode, NbtLimit};
use crate::components::holder::registry_by_name_codec;
use crate::components::map::DataComponentExactPredicate;
use mc_data::{DataComponentPredicateType as Pt, DataComponentType};
use mc_dfu::{Codec, DataResult, Either, MapCodec};
use std::sync::LazyLock;

/// `DataComponentPredicate.Type`: a registered predicate type, or `AnyValueType` (a component that merely has to exist).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateType {
    Concrete(Pt),
    AnyValue(DataComponentType),
}

/// One predicate of `DataComponentMatchers.partial` (vanilla's `Map<Type, DataComponentPredicate>` entry; the type
/// is determined by the value).
#[derive(Clone, Debug, PartialEq)]
pub enum DataComponentPredicate {
    Damage(DamagePredicate),
    Enchantments(EnchantmentsPredicate),
    StoredEnchantments(EnchantmentsPredicate),
    PotionContents(PotionsPredicate),
    CustomData(CustomDataPredicate),
    Container(ContainerPredicate),
    BundleContents(BundlePredicate),
    FireworkExplosion(FireworkExplosionPredicate),
    Fireworks(FireworksPredicate),
    WritableBookContent(WritableBookPredicate),
    WrittenBookContent(WrittenBookPredicate),
    AttributeModifiers(AttributeModifiersPredicate),
    Trim(TrimPredicate),
    JukeboxPlayable(JukeboxPlayablePredicate),
    VillagerVariant(VillagerTypePredicate),
    /// `AnyValue(componentType)`.
    AnyValue(DataComponentType),
}

impl DataComponentPredicate {
    pub fn predicate_type(&self) -> PredicateType {
        use DataComponentPredicate as P;
        PredicateType::Concrete(match self {
            P::Damage(_) => Pt::DAMAGE,
            P::Enchantments(_) => Pt::ENCHANTMENTS,
            P::StoredEnchantments(_) => Pt::STORED_ENCHANTMENTS,
            P::PotionContents(_) => Pt::POTION_CONTENTS,
            P::CustomData(_) => Pt::CUSTOM_DATA,
            P::Container(_) => Pt::CONTAINER,
            P::BundleContents(_) => Pt::BUNDLE_CONTENTS,
            P::FireworkExplosion(_) => Pt::FIREWORK_EXPLOSION,
            P::Fireworks(_) => Pt::FIREWORKS,
            P::WritableBookContent(_) => Pt::WRITABLE_BOOK_CONTENT,
            P::WrittenBookContent(_) => Pt::WRITTEN_BOOK_CONTENT,
            P::AttributeModifiers(_) => Pt::ATTRIBUTE_MODIFIERS,
            P::Trim(_) => Pt::TRIM,
            P::JukeboxPlayable(_) => Pt::JUKEBOX_PLAYABLE,
            P::VillagerVariant(_) => Pt::VILLAGER_VARIANT,
            P::AnyValue(c) => return PredicateType::AnyValue(*c),
        })
    }
}

impl PredicateType {
    /// `DataComponentPredicate.Type.CODEC`: a predicate type name, else a component type name.
    pub fn codec() -> Codec<PredicateType> {
        registry_by_name_codec::<Pt>().either(&registry_by_name_codec::<DataComponentType>()).xmap(
            |e| match e {
                Either::Left(t) => PredicateType::Concrete(t),
                Either::Right(c) => PredicateType::AnyValue(c),
            },
            |t: &PredicateType| match t {
                PredicateType::Concrete(t) => Either::Left(*t),
                PredicateType::AnyValue(c) => Either::Right(*c),
            },
        )
    }

    /// The codec of this type's predicate (`Type.codec()`).
    pub fn value_codec(self) -> Codec<DataComponentPredicate> {
        static CODECS: LazyLock<Vec<Codec<DataComponentPredicate>>> = LazyLock::new(|| {
            use DataComponentPredicate as P;
            // registry order of `DataComponentPredicates`
            let codecs = vec![
                (Pt::DAMAGE, DamagePredicate::codec().xmap(P::Damage, |p: &P| match p { P::Damage(v) => v.clone(), _ => unreachable!() })),
                (Pt::ENCHANTMENTS, EnchantmentsPredicate::codec().xmap(P::Enchantments, |p: &P| match p { P::Enchantments(v) => v.clone(), _ => unreachable!() })),
                (Pt::STORED_ENCHANTMENTS, EnchantmentsPredicate::codec().xmap(P::StoredEnchantments, |p: &P| match p { P::StoredEnchantments(v) => v.clone(), _ => unreachable!() })),
                (Pt::POTION_CONTENTS, PotionsPredicate::codec().xmap(P::PotionContents, |p: &P| match p { P::PotionContents(v) => v.clone(), _ => unreachable!() })),
                (Pt::CUSTOM_DATA, CustomDataPredicate::codec().xmap(P::CustomData, |p: &P| match p { P::CustomData(v) => v.clone(), _ => unreachable!() })),
                (Pt::CONTAINER, ContainerPredicate::codec().xmap(P::Container, |p: &P| match p { P::Container(v) => v.clone(), _ => unreachable!() })),
                (Pt::BUNDLE_CONTENTS, BundlePredicate::codec().xmap(P::BundleContents, |p: &P| match p { P::BundleContents(v) => v.clone(), _ => unreachable!() })),
                (Pt::FIREWORK_EXPLOSION, FireworkExplosionPredicate::codec().xmap(P::FireworkExplosion, |p: &P| match p { P::FireworkExplosion(v) => v.clone(), _ => unreachable!() })),
                (Pt::FIREWORKS, FireworksPredicate::codec().xmap(P::Fireworks, |p: &P| match p { P::Fireworks(v) => v.clone(), _ => unreachable!() })),
                (Pt::WRITABLE_BOOK_CONTENT, WritableBookPredicate::codec().xmap(P::WritableBookContent, |p: &P| match p { P::WritableBookContent(v) => v.clone(), _ => unreachable!() })),
                (Pt::WRITTEN_BOOK_CONTENT, WrittenBookPredicate::codec().xmap(P::WrittenBookContent, |p: &P| match p { P::WrittenBookContent(v) => v.clone(), _ => unreachable!() })),
                (Pt::ATTRIBUTE_MODIFIERS, AttributeModifiersPredicate::codec().xmap(P::AttributeModifiers, |p: &P| match p { P::AttributeModifiers(v) => v.clone(), _ => unreachable!() })),
                (Pt::TRIM, TrimPredicate::codec().xmap(P::Trim, |p: &P| match p { P::Trim(v) => v.clone(), _ => unreachable!() })),
                (Pt::JUKEBOX_PLAYABLE, JukeboxPlayablePredicate::codec().xmap(P::JukeboxPlayable, |p: &P| match p { P::JukeboxPlayable(v) => v.clone(), _ => unreachable!() })),
                (Pt::VILLAGER_VARIANT, VillagerTypePredicate::codec().xmap(P::VillagerVariant, |p: &P| match p { P::VillagerVariant(v) => v.clone(), _ => unreachable!() })),
            ];
            assert!(codecs.iter().enumerate().all(|(i, (t, _))| t.id() as usize == i), "predicate type order");
            codecs.into_iter().map(|(_, c)| c).collect()
        });
        match self {
            PredicateType::Concrete(t) => CODECS[t.id() as usize].clone(),
            // `MapCodec.unitCodec(AnyValue)`
            PredicateType::AnyValue(c) => Codec::unit(DataComponentPredicate::AnyValue(c)),
        }
    }

    /// `Type.STREAM_CODEC`: `either(registry(predicate type), registry(component type))`.
    fn encode_stream(self, out: &mut Vec<u8>) -> EncodeResult<()> {
        match self {
            PredicateType::Concrete(t) => {
                true.encode(out)?;
                t.encode(out)?;
            }
            PredicateType::AnyValue(c) => {
                false.encode(out)?;
                c.encode(out)?;
            }
        };
        Ok(())
    }
    fn decode_stream(input: &mut &[u8]) -> DecodeResult<PredicateType> {
        if bool::decode(input)? {
            Pt::decode(input).map(PredicateType::Concrete)
        } else {
            DataComponentType::decode(input).map(PredicateType::AnyValue)
        }
    }
}

impl DataComponentPredicate {
    /// `DataComponentPredicate.SINGLE_STREAM_CODEC`: the type, then the predicate as an NBT tag
    /// (`ByteBufCodecs.fromCodecWithRegistries(codec)`).
    pub fn encode_single(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        let ty = self.predicate_type();
        ty.encode_stream(out)?;
        let tag = mc_dfu::encode_nbt(&ty.value_codec(), self)
            .into_result()
            .map_err(|e| crate::codec::EncodeError(format!("Failed to encode: {e}")))?;
        crate::codec::encode_tag(&tag, out)
    }

    pub fn decode_single(input: &mut &[u8]) -> DecodeResult<DataComponentPredicate> {
        let ty = PredicateType::decode_stream(input)?;
        let tag = crate::codec::decode_tag_with(input, NbtLimit::DEFAULT)?
            .ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))?;
        match mc_dfu::parse_nbt(&ty.value_codec(), &tag) {
            DataResult::Success(p) => Ok(p),
            e => Err(DecodeError::Message(format!("Failed to decode: {} {tag}", e.error_message().unwrap_or_default()))),
        }
    }
}

/// `Map<DataComponentPredicate.Type<?>, DataComponentPredicate>` in entry order; equality ignores the order like `Map.equals`.
#[derive(Clone, Debug, Default)]
pub struct PredicateMap(pub Vec<DataComponentPredicate>);

impl PartialEq for PredicateMap {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().all(|p| other.0.iter().any(|q| p == q))
    }
}

impl PredicateMap {
    /// `DataComponentPredicate.CODEC`: `Codec.dispatchedMap(Type.CODEC, Type::codec)`.
    pub fn codec() -> Codec<PredicateMap> {
        static C: LazyLock<Codec<PredicateMap>> = LazyLock::new(|| {
            mc_dfu::extra::dispatched_map(&PredicateType::codec(), |t: &PredicateType| DataResult::success(t.value_codec())).xmap(
                |entries: Vec<(PredicateType, DataComponentPredicate)>| PredicateMap(entries.into_iter().map(|(_, p)| p).collect()),
                |m: &PredicateMap| m.0.iter().map(|p| (p.predicate_type(), p.clone())).collect(),
            )
        });
        C.clone()
    }
}

/// `ByteBufCodecs.list(64)` of singles, folded by `Collectors.toMap` (duplicate types fail; `AnyValueType`s are
/// distinct objects and never collide).
impl Encode for PredicateMap {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        crate::codec::write_count(out, self.0.len(), 64)?;
        for p in &self.0 {
            p.encode_single(out)?;
        }
        Ok(())
    }
}
impl Decode for PredicateMap {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, 64)?;
        let mut entries: Vec<DataComponentPredicate> = Vec::with_capacity(count);
        for _ in 0..count {
            let p = DataComponentPredicate::decode_single(input)?;
            if matches!(p.predicate_type(), PredicateType::Concrete(_)) && entries.iter().any(|q| q.predicate_type() == p.predicate_type()) {
                return Err(DecodeError::Message(format!("Duplicate key {:?}", p.predicate_type())));
            }
            entries.push(p);
        }
        Ok(PredicateMap(entries))
    }
}

/// `DataComponentMatchers`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DataComponentMatchers {
    pub exact: DataComponentExactPredicate,
    pub partial: PredicateMap,
}

impl DataComponentMatchers {
    /// `DataComponentMatchers.ANY`.
    pub fn any() -> Self {
        Self::default()
    }

    /// `DataComponentMatchers.CODEC` (a `MapCodec`: the `components` and `predicates` fields of the enclosing object).
    pub fn map_codec() -> MapCodec<DataComponentMatchers> {
        mc_dfu::record!(DataComponentMatchers {
            exact: DataComponentExactPredicate::codec().optional_field_of_default("components", DataComponentExactPredicate::empty()),
            partial: Codec::lazy(PredicateMap::codec).optional_field_of_default("predicates", PredicateMap::default()),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.partial.0.is_empty()
    }
}

impl Encode for DataComponentMatchers {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.exact.encode(out)?;
        self.partial.encode(out)?;
        Ok(())
    }
}
impl Decode for DataComponentMatchers {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(DataComponentMatchers { exact: DataComponentExactPredicate::decode(input)?, partial: PredicateMap::decode(input)? })
    }
}
