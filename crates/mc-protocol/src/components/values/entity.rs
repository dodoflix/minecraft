//! Entity-related components: `entity_data` / `block_entity_data` (`TypedEntityData`) and the variant components of
//! mobs (registry-backed `Holder`s and the `StringRepresentable` variant enums with their `ByIdMap` stream forms).
use crate::codec::EncodeResult;
use crate::codec::{Decode, DecodeResult, Encode, read_varint, write_varint};
use crate::components::common::{custom_data_compound_tag_codec, read_enum_zero};
use crate::components::component::{ComponentDef, ComponentKey};
use crate::components::holder::{self, Holder};
use crate::packets::play::PaintingVariant;
use crate::string_enum;
use mc_data::DataComponentType;
use mc_dfu::{Codec, DataResult, Dyn, NBT};
use mc_nbt::CompoundTag;

// ---- TypedEntityData -------------------------------------------------------------------------------------------

/// `TypedEntityData<IdType>`: a type id and its compound data (the `id` key is stripped from the data).
#[derive(Clone, Debug, PartialEq)]
pub struct TypedEntityData<T> {
    pub ty: T,
    /// Never contains `id`.
    pub tag: CompoundTag,
}

impl<T> TypedEntityData<T> {
    /// `TypedEntityData.of(type, data)` (strips `id`).
    pub fn of(ty: T, mut tag: CompoundTag) -> Self {
        tag.remove("id");
        TypedEntityData { ty, tag }
    }
}

impl<T: Clone + Send + Sync + 'static> TypedEntityData<T> {
    /// `TypedEntityData.codec(typeCodec)`: the compound with the type flattened into its `id` key.
    pub fn codec(type_codec: &Codec<T>) -> Codec<TypedEntityData<T>> {
        let (tc_enc, tc_dec) = (type_codec.clone(), type_codec.clone());
        let (compound_enc, compound_dec) = (
            custom_data_compound_tag_codec(),
            custom_data_compound_tag_codec(),
        );
        Codec::from_fns(
            move |ops, input: &TypedEntityData<T>, prefix| {
                let compound_enc = compound_enc.clone();
                tc_enc
                    .encode_start(&NBT, &input.ty)
                    .flat_map(move |d: Dyn| match d.to_tag() {
                        Ok(type_tag) => {
                            let mut tag = input.tag.clone();
                            tag.put("id", type_tag);
                            compound_enc.encode(ops, &tag, prefix)
                        }
                        Err(e) => DataResult::error(move || e.clone()),
                    })
            },
            move |ops, input| {
                let tc_dec = tc_dec.clone();
                compound_dec
                    .decode(ops, input)
                    .flat_map(move |(mut tag, rest)| match tag.remove("id") {
                        None => {
                            let shown = input.to_string();
                            DataResult::error(move || format!("Expected 'id' field in {shown}"))
                        }
                        Some(type_tag) => tc_dec
                            .parse(&NBT, &Dyn::from(&type_tag))
                            .map(move |ty| (TypedEntityData { ty, tag }, rest)),
                    })
            },
        )
    }
}

/// `TypedEntityData.streamCodec(typeCodec)` with the type's own `Encode`/`Decode`.
impl<T: Encode> Encode for TypedEntityData<T> {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.ty.encode(out)?;
        self.tag.encode(out)?;
        Ok(())
    }
}
impl<T: Decode> Decode for TypedEntityData<T> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let ty = T::decode(input)?;
        Ok(TypedEntityData::of(ty, CompoundTag::decode(input)?))
    }
}

pub const ENTITY_DATA: ComponentKey<TypedEntityData<mc_data::EntityType>> =
    ComponentKey::new(DataComponentType::ENTITY_DATA);
pub const BLOCK_ENTITY_DATA: ComponentKey<TypedEntityData<mc_data::BlockEntityType>> =
    ComponentKey::new(DataComponentType::BLOCK_ENTITY_DATA);

// ---- registry-backed variants ----------------------------------------------------------------------------------

macro_rules! holder_variants {
    ($($(#[$m:meta])* $ty:ident $key:ident $registry:literal;)+) => {$(
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq)]
        pub enum $ty {}
        pub const $key: ComponentKey<Holder<$ty>> = ComponentKey::new(DataComponentType::$key);
    )+
    fn holder_defs(out: &mut Vec<ComponentDef>) {
        $(out.push(
            ComponentDef::builder($key)
                .persistent(holder::holder_fixed_codec::<$ty>($registry))
                .network_with(
                    |h, out| holder::encode_holder_registry($registry, h, out),
                    |input| holder::decode_holder_registry($registry, input),
                )
                .build(),
        );)+
    }};
}

// Uninhabited markers for `Holder<X>` entries: these registries are referenced by name only (`RegistryFixedCodec`).
holder_variants! {
    VillagerType VILLAGER_VARIANT "minecraft:villager_type";
    WolfVariant WOLF_VARIANT "minecraft:wolf_variant";
    WolfSoundVariant WOLF_SOUND_VARIANT "minecraft:wolf_sound_variant";
    PigVariant PIG_VARIANT "minecraft:pig_variant";
    PigSoundVariant PIG_SOUND_VARIANT "minecraft:pig_sound_variant";
    CowVariant COW_VARIANT "minecraft:cow_variant";
    CowSoundVariant COW_SOUND_VARIANT "minecraft:cow_sound_variant";
    ChickenVariant CHICKEN_VARIANT "minecraft:chicken_variant";
    ChickenSoundVariant CHICKEN_SOUND_VARIANT "minecraft:chicken_sound_variant";
    ZombieNautilusVariant ZOMBIE_NAUTILUS_VARIANT "minecraft:zombie_nautilus_variant";
    FrogVariant FROG_VARIANT "minecraft:frog_variant";
    CatVariant CAT_VARIANT "minecraft:cat_variant";
    CatSoundVariant CAT_SOUND_VARIANT "minecraft:cat_sound_variant";
}

/// `painting/variant`: persistent as a reference, stream form allows an inline `PaintingVariant` (`ByteBufCodecs.holder`).
pub const PAINTING_VARIANT: ComponentKey<Holder<PaintingVariant>> =
    ComponentKey::new(DataComponentType::PAINTING_VARIANT);

// ---- StringRepresentable variant enums -------------------------------------------------------------------------

/// Stream form `ByteBufCodecs.idMapper(BY_ID, getId)` of a continuous-id enum; `$oob` is the `ByIdMap` strategy.
macro_rules! id_mapped {
    ($ty:ident, zero) => {
        id_mapped!(@impl $ty, |input: &mut &[u8]| read_enum_zero(input, $ty::ALL));
    };
    ($ty:ident, clamp) => {
        id_mapped!(@impl $ty, |input: &mut &[u8]| {
            let id = read_varint(input)?;
            Ok($ty::ALL[id.clamp(0, $ty::ALL.len() as i32 - 1) as usize])
        });
    };
    ($ty:ident, wrap) => {
        id_mapped!(@impl $ty, |input: &mut &[u8]| {
            let id = read_varint(input)?;
            Ok($ty::ALL[id.rem_euclid($ty::ALL.len() as i32) as usize])
        });
    };
    (@impl $ty:ident, $read:expr) => {
        impl Encode for $ty {
            fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
                write_varint(out, self.id());
                Ok(())
            }
        }
        impl Decode for $ty {
            fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
                ($read)(input)
            }
        }
    };
}

string_enum! {
    /// `Fox.Variant`.
    pub enum FoxVariant { Red = "red", Snow = "snow" }
}
id_mapped!(FoxVariant, zero);

string_enum! {
    /// `Salmon.Variant`.
    pub enum SalmonVariant { Small = "small", Medium = "medium", Large = "large" }
}
id_mapped!(SalmonVariant, clamp);

string_enum! {
    /// `Parrot.Variant`.
    pub enum ParrotVariant { RedBlue = "red_blue", Blue = "blue", Green = "green", YellowBlue = "yellow_blue", Gray = "gray" }
}
id_mapped!(ParrotVariant, clamp);

string_enum! {
    /// `MushroomCow.Variant`.
    pub enum MushroomCowVariant { Red = "red", Brown = "brown" }
}
id_mapped!(MushroomCowVariant, clamp);

string_enum! {
    /// `Variant` of horses (`world.entity.animal.equine.Variant`).
    pub enum HorseVariant {
        White = "white", Creamy = "creamy", Chestnut = "chestnut", Brown = "brown", Black = "black", Gray = "gray",
        DarkBrown = "dark_brown",
    }
}
id_mapped!(HorseVariant, wrap);

string_enum! {
    /// `Llama.Variant`.
    pub enum LlamaVariant { Creamy = "creamy", White = "white", Brown = "brown", Gray = "gray" }
}
id_mapped!(LlamaVariant, clamp);

string_enum! {
    /// `Axolotl.Variant`.
    pub enum AxolotlVariant { Lucy = "lucy", Wild = "wild", Gold = "gold", Cyan = "cyan", Blue = "blue" }
}
id_mapped!(AxolotlVariant, zero);

string_enum! {
    /// `Rabbit.Variant` (ids are sparse: `evil` is 99).
    pub enum RabbitVariant {
        Brown = "brown", White = "white", Black = "black", WhiteSplotched = "white_splotched", Gold = "gold", Salt = "salt",
        Evil = "evil",
    }
}

impl RabbitVariant {
    /// `Rabbit.Variant.id()`.
    pub fn wire_id(self) -> i32 {
        if self == RabbitVariant::Evil {
            99
        } else {
            self.id()
        }
    }
}
impl Encode for RabbitVariant {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.wire_id());
        Ok(())
    }
}
impl Decode for RabbitVariant {
    /// `ByIdMap.sparse(.., DEFAULT = BROWN)`.
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let id = read_varint(input)?;
        Ok(RabbitVariant::ALL
            .iter()
            .copied()
            .find(|v| v.wire_id() == id)
            .unwrap_or(RabbitVariant::Brown))
    }
}

string_enum! {
    /// `TropicalFish.Pattern`.
    pub enum TropicalFishPattern {
        Kob = "kob", Sunstreak = "sunstreak", Snooper = "snooper", Dasher = "dasher", Brinely = "brinely", Spotty = "spotty",
        Flopper = "flopper", Stripey = "stripey", Glitter = "glitter", Blockfish = "blockfish", Betty = "betty",
        Clayfish = "clayfish",
    }
}

impl TropicalFishPattern {
    /// `getPackedId()`: base id (small 0, large 1) | index << 8.
    pub fn packed_id(self) -> i32 {
        let ordinal = self.id();
        let (base, index) = if ordinal < 6 {
            (0, ordinal)
        } else {
            (1, ordinal - 6)
        };
        base | index << 8
    }
}
impl Encode for TropicalFishPattern {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.packed_id());
        Ok(())
    }
}
impl Decode for TropicalFishPattern {
    /// `ByIdMap.sparse(.., KOB)`.
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let id = read_varint(input)?;
        Ok(TropicalFishPattern::ALL
            .iter()
            .copied()
            .find(|v| v.packed_id() == id)
            .unwrap_or(TropicalFishPattern::Kob))
    }
}

pub const FOX_VARIANT: ComponentKey<FoxVariant> = ComponentKey::new(DataComponentType::FOX_VARIANT);
pub const SALMON_SIZE: ComponentKey<SalmonVariant> =
    ComponentKey::new(DataComponentType::SALMON_SIZE);
pub const PARROT_VARIANT: ComponentKey<ParrotVariant> =
    ComponentKey::new(DataComponentType::PARROT_VARIANT);
pub const TROPICAL_FISH_PATTERN: ComponentKey<TropicalFishPattern> =
    ComponentKey::new(DataComponentType::TROPICAL_FISH_PATTERN);
pub const MOOSHROOM_VARIANT: ComponentKey<MushroomCowVariant> =
    ComponentKey::new(DataComponentType::MOOSHROOM_VARIANT);
pub const RABBIT_VARIANT: ComponentKey<RabbitVariant> =
    ComponentKey::new(DataComponentType::RABBIT_VARIANT);
pub const HORSE_VARIANT: ComponentKey<HorseVariant> =
    ComponentKey::new(DataComponentType::HORSE_VARIANT);
pub const LLAMA_VARIANT: ComponentKey<LlamaVariant> =
    ComponentKey::new(DataComponentType::LLAMA_VARIANT);
pub const AXOLOTL_VARIANT: ComponentKey<AxolotlVariant> =
    ComponentKey::new(DataComponentType::AXOLOTL_VARIANT);

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(
        ComponentDef::builder(ENTITY_DATA)
            .persistent(TypedEntityData::codec(&holder::registry_by_name_codec::<
                mc_data::EntityType,
            >()))
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(BLOCK_ENTITY_DATA)
            .persistent(TypedEntityData::codec(&holder::registry_by_name_codec::<
                mc_data::BlockEntityType,
            >()))
            .network_synchronized()
            .build(),
    );
    holder_defs(out);
    out.push(
        ComponentDef::builder(PAINTING_VARIANT)
            .persistent(holder::holder_fixed_codec::<PaintingVariant>(
                "minecraft:painting_variant",
            ))
            .network_with(
                |h, out| {
                    holder::encode_holder("minecraft:painting_variant", h, out, |v, o| v.encode(o))
                },
                |input| {
                    holder::decode_holder(
                        "minecraft:painting_variant",
                        input,
                        PaintingVariant::decode,
                    )
                },
            )
            .build(),
    );
    macro_rules! enums {
        ($($key:ident $ty:ident),+) => {$(
            out.push(ComponentDef::builder($key).persistent($ty::codec()).network_synchronized().build());
        )+};
    }
    enums!(
        FOX_VARIANT FoxVariant, SALMON_SIZE SalmonVariant, PARROT_VARIANT ParrotVariant,
        TROPICAL_FISH_PATTERN TropicalFishPattern, MOOSHROOM_VARIANT MushroomCowVariant, RABBIT_VARIANT RabbitVariant,
        HORSE_VARIANT HorseVariant, LLAMA_VARIANT LlamaVariant, AXOLOTL_VARIANT AxolotlVariant
    );
}
