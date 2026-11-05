//! Decoration-like components: books, fireworks, banner patterns, pot decorations, sign text, instruments, jukebox
//! songs, armor trims, `provides_*`.

use super::containers::{read_list, read_optional, write_list, write_optional};
use crate::codec::{
    Decode, DecodeError, DecodeResult, Encode, MAX_STRING_LENGTH, NbtLimit, decode_tag_with,
    encode_tag, read_utf, read_varint, write_utf, write_varint,
};
use crate::components::common::{DyeColor, SoundEvent, component_codec, read_enum_zero};
use crate::components::component::{ComponentDef, ComponentKey};
use crate::components::holder::{self, Holder, HolderSet, NoDirect};
use crate::components::item_stack::ItemStackTemplate;
use crate::string_enum;
use mc_core::ident::Identifier;
use mc_data::DataComponentType;
use mc_dfu::extra::{
    IDENTIFIER, NON_NEGATIVE_FLOAT, NON_NEGATIVE_INT, POSITIVE_FLOAT, UNSIGNED_BYTE,
    extra_int_range, optional_always_present_field_of,
};
use mc_dfu::{BOOL, Codec, DataResult, INT, STRING, record, string_range};
use mc_text::Component;

pub const WRITABLE_BOOK_CONTENT: ComponentKey<WritableBookContent> =
    ComponentKey::new(DataComponentType::WRITABLE_BOOK_CONTENT);
pub const WRITTEN_BOOK_CONTENT: ComponentKey<WrittenBookContent> =
    ComponentKey::new(DataComponentType::WRITTEN_BOOK_CONTENT);
pub const FIREWORK_EXPLOSION: ComponentKey<FireworkExplosion> =
    ComponentKey::new(DataComponentType::FIREWORK_EXPLOSION);
pub const FIREWORKS: ComponentKey<Fireworks> = ComponentKey::new(DataComponentType::FIREWORKS);
pub const BANNER_PATTERNS: ComponentKey<BannerPatternLayers> =
    ComponentKey::new(DataComponentType::BANNER_PATTERNS);
pub const POT_DECORATIONS: ComponentKey<PotDecorations> =
    ComponentKey::new(DataComponentType::POT_DECORATIONS);
pub const SIGN_TEXT_FRONT: ComponentKey<SignText> =
    ComponentKey::new(DataComponentType::SIGN_TEXT_FRONT);
pub const SIGN_TEXT_BACK: ComponentKey<SignText> =
    ComponentKey::new(DataComponentType::SIGN_TEXT_BACK);
pub const INSTRUMENT: ComponentKey<InstrumentComponent> =
    ComponentKey::new(DataComponentType::INSTRUMENT);
pub const JUKEBOX_PLAYABLE: ComponentKey<JukeboxPlayable> =
    ComponentKey::new(DataComponentType::JUKEBOX_PLAYABLE);
pub const TRIM: ComponentKey<ArmorTrim> = ComponentKey::new(DataComponentType::TRIM);
pub const PROVIDES_TRIM_MATERIAL: ComponentKey<Holder<TrimMaterial>> =
    ComponentKey::new(DataComponentType::PROVIDES_TRIM_MATERIAL);
pub const PROVIDES_BANNER_PATTERNS: ComponentKey<HolderSet<BannerPattern>> =
    ComponentKey::new(DataComponentType::PROVIDES_BANNER_PATTERNS);
pub const PROVIDES_POTTERY_PATTERN: ComponentKey<Holder<DecoratedPotPattern>> =
    ComponentKey::new(DataComponentType::PROVIDES_POTTERY_PATTERN);

const BANNER_PATTERN: &str = "minecraft:banner_pattern";
const TRIM_MATERIAL_REG: &str = "minecraft:trim_material";
const TRIM_PATTERN_REG: &str = "minecraft:trim_pattern";
const INSTRUMENT_REG: &str = "minecraft:instrument";
const JUKEBOX_SONG_REG: &str = "minecraft:jukebox_song";
const DECORATED_POT_PATTERN: &str = "minecraft:decorated_pot_pattern";

// ---- stream helpers ----------------------------------------------------------------------------------------------

/// `ComponentSerialization.STREAM_CODEC` (`fromCodecWithRegistries`: default NBT quota).
fn encode_component(c: &Component, out: &mut Vec<u8>) {
    match c.to_nbt() {
        Ok(tag) => encode_tag(&tag, out),
        Err(e) => panic!("Failed to encode: {e} {c:?}"),
    }
}
fn decode_component(input: &mut &[u8]) -> DecodeResult<Component> {
    let tag = decode_tag_with(input, NbtLimit::DEFAULT)?
        .ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))?;
    Component::from_nbt(&tag)
        .map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {tag}")))
}

fn encode_utf(s: &String, out: &mut Vec<u8>) {
    write_utf(out, s)
}

/// `ComponentSerialization.flatRestrictedCodec(max)`.
fn flat_restricted_codec(max: usize) -> Codec<Component> {
    let (e, d) = (component_codec(), component_codec());
    Codec::from_fns(
        move |ops, input: &Component, _prefix| e.encode_start(ops, input),
        move |ops, input| {
            d.decode(ops, input).flat_map(|(c, rest)| {
                if c.to_json()
                    .is_ok_and(|j| mc_text::util::encodes_longer_than(&j, max))
                {
                    DataResult::error(move || {
                        format!("Component was too large: greater than max size {max}")
                    })
                } else {
                    DataResult::success((c, rest))
                }
            })
        },
    )
}

// ---- Filterable ----------------------------------------------------------------------------------------------------

/// `net.minecraft.server.network.Filterable`: a raw value and an optional filtered replacement.
#[derive(Clone, Debug, PartialEq)]
pub struct Filterable<T> {
    pub raw: T,
    pub filtered: Option<T>,
}

impl<T> Filterable<T> {
    pub fn pass_through(raw: T) -> Filterable<T> {
        Filterable {
            raw,
            filtered: None,
        }
    }
    /// `get(filterEnabled)`.
    pub fn get(&self, filter_enabled: bool) -> &T {
        if filter_enabled {
            self.filtered.as_ref().unwrap_or(&self.raw)
        } else {
            &self.raw
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Filterable<T> {
    /// `Filterable.codec(valueCodec)`: `{raw, filtered?}` or the bare value.
    pub fn codec(value: &Codec<T>) -> Codec<Filterable<T>> {
        let full = value
            .field_of("raw")
            .pair(&value.optional_field_of("filtered"))
            .codec()
            .xmap(
                |(raw, filtered)| Filterable { raw, filtered },
                |f: &Filterable<T>| (f.raw.clone(), f.filtered.clone()),
            );
        let simple = value.xmap(Filterable::pass_through, |f: &Filterable<T>| f.raw.clone());
        full.with_alternative(&simple)
    }
}

impl<T> Filterable<T> {
    /// `Filterable.streamCodec(valueCodec)`.
    pub fn encode_stream(&self, out: &mut Vec<u8>, f: impl Fn(&T, &mut Vec<u8>)) {
        f(&self.raw, out);
        write_optional(out, &self.filtered, |v, o| f(v, o));
    }
    pub fn decode_stream(
        input: &mut &[u8],
        f: impl Fn(&mut &[u8]) -> DecodeResult<T>,
    ) -> DecodeResult<Filterable<T>> {
        let raw = f(input)?;
        Ok(Filterable {
            raw,
            filtered: read_optional(input, &f)?,
        })
    }
}

// ---- books -------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Default)]
pub struct WritableBookContent {
    pub pages: Vec<Filterable<String>>,
}

impl WritableBookContent {
    pub const PAGE_EDIT_LENGTH: usize = 1024;
    pub const MAX_PAGES: usize = 100;

    /// `WritableBookContent.PAGES_CODEC`.
    pub fn pages_codec() -> Codec<Vec<Filterable<String>>> {
        Filterable::codec(&string_range(0, 1024)).size_limited_list_of(100)
    }

    pub fn codec() -> Codec<WritableBookContent> {
        let page = Filterable::codec(&string_range(0, 1024));
        record!(WritableBookContent {
            pages: page
                .size_limited_list_of(100)
                .optional_field_of_default("pages", Vec::new())
        })
        .codec()
    }
}

impl Encode for WritableBookContent {
    fn encode(&self, out: &mut Vec<u8>) {
        write_list(out, &self.pages, |p, o| p.encode_stream(o, encode_utf));
    }
}
impl Decode for WritableBookContent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_list(input, 100, |i| {
            Filterable::decode_stream(i, |i| read_utf(i, 1024))
        })
        .map(|pages| WritableBookContent { pages })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WrittenBookContent {
    pub title: Filterable<String>,
    pub author: String,
    pub generation: i32,
    pub pages: Vec<Filterable<Component>>,
    pub resolved: bool,
}

impl Default for WrittenBookContent {
    /// `WrittenBookContent.EMPTY`.
    fn default() -> Self {
        WrittenBookContent {
            title: Filterable::pass_through(String::new()),
            author: String::new(),
            generation: 0,
            pages: Vec::new(),
            resolved: true,
        }
    }
}

impl WrittenBookContent {
    pub const PAGE_LENGTH: usize = 32767;
    pub const TITLE_MAX_LENGTH: usize = 32;
    pub const MAX_GENERATION: i32 = 3;

    /// `WrittenBookContent.PAGES_CODEC`.
    pub fn pages_codec() -> Codec<Vec<Filterable<Component>>> {
        Filterable::codec(&flat_restricted_codec(32767)).list_of()
    }

    pub fn codec() -> Codec<WrittenBookContent> {
        let pages = Filterable::codec(&flat_restricted_codec(32767)).list_of();
        record!(WrittenBookContent {
            title: Filterable::codec(&string_range(0, 32)).field_of("title"),
            author: STRING.field_of("author"),
            generation: extra_int_range(0, 3).optional_field_of_default("generation", 0),
            pages: pages.optional_field_of_default("pages", Vec::new()),
            resolved: BOOL.optional_field_of_default("resolved", false),
        })
        .codec()
    }
}

impl Encode for WrittenBookContent {
    fn encode(&self, out: &mut Vec<u8>) {
        self.title.encode_stream(out, encode_utf);
        write_utf(out, &self.author);
        write_varint(out, self.generation);
        write_list(out, &self.pages, |p, o| {
            p.encode_stream(o, encode_component)
        });
        self.resolved.encode(out);
    }
}
impl Decode for WrittenBookContent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let title = Filterable::decode_stream(input, |i| read_utf(i, 32))?;
        let author = read_utf(input, MAX_STRING_LENGTH)?;
        let generation = read_varint(input)?;
        let pages = read_list(input, i32::MAX as usize, |i| {
            Filterable::decode_stream(i, decode_component)
        })?;
        let resolved = bool::decode(input)?;
        if !(0..=3).contains(&generation) {
            return Err(DecodeError::Message(format!(
                "Generation was {generation}, but must be between 0 and 3"
            )));
        }
        Ok(WrittenBookContent {
            title,
            author,
            generation,
            pages,
            resolved,
        })
    }
}

// ---- fireworks ---------------------------------------------------------------------------------------------------------

string_enum! {
    /// `FireworkExplosion.Shape`.
    pub enum FireworkShape { SmallBall = "small_ball", LargeBall = "large_ball", Star = "star", Creeper = "creeper", Burst = "burst" }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FireworkExplosion {
    pub shape: FireworkShape,
    pub colors: Vec<i32>,
    pub fade_colors: Vec<i32>,
    pub has_trail: bool,
    pub has_twinkle: bool,
}

impl Default for FireworkExplosion {
    /// `FireworkExplosion.DEFAULT`.
    fn default() -> Self {
        FireworkExplosion {
            shape: FireworkShape::SmallBall,
            colors: Vec::new(),
            fade_colors: Vec::new(),
            has_trail: false,
            has_twinkle: false,
        }
    }
}

impl FireworkExplosion {
    pub fn codec() -> Codec<FireworkExplosion> {
        let list = INT.list_of();
        record!(FireworkExplosion {
            shape: FireworkShape::codec().field_of("shape"),
            colors: list.optional_field_of_default("colors", Vec::new()),
            fade_colors: list.optional_field_of_default("fade_colors", Vec::new()),
            has_trail: BOOL.optional_field_of_default("has_trail", false),
            has_twinkle: BOOL.optional_field_of_default("has_twinkle", false),
        })
        .codec()
    }
}

fn write_colors(v: &[i32], out: &mut Vec<u8>) {
    write_list(out, v, |c, o| c.encode(o));
}
fn read_colors(input: &mut &[u8]) -> DecodeResult<Vec<i32>> {
    read_list(input, i32::MAX as usize, i32::decode)
}

impl Encode for FireworkExplosion {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.shape.id());
        write_colors(&self.colors, out);
        write_colors(&self.fade_colors, out);
        self.has_trail.encode(out);
        self.has_twinkle.encode(out);
    }
}
impl Decode for FireworkExplosion {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(FireworkExplosion {
            shape: read_enum_zero(input, FireworkShape::ALL)?,
            colors: read_colors(input)?,
            fade_colors: read_colors(input)?,
            has_trail: bool::decode(input)?,
            has_twinkle: bool::decode(input)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Fireworks {
    pub flight_duration: i32,
    pub explosions: Vec<FireworkExplosion>,
}

impl Fireworks {
    pub const MAX_EXPLOSIONS: usize = 256;

    pub fn codec() -> Codec<Fireworks> {
        record!(Fireworks {
            flight_duration: UNSIGNED_BYTE.optional_field_of_default("flight_duration", 0),
            explosions: FireworkExplosion::codec()
                .size_limited_list_of(256)
                .optional_field_of_default("explosions", Vec::new()),
        })
        .codec()
    }
}

impl Encode for Fireworks {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.flight_duration);
        write_list(out, &self.explosions, |e, o| e.encode(o));
    }
}
impl Decode for Fireworks {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let flight_duration = read_varint(input)?;
        Ok(Fireworks {
            flight_duration,
            explosions: read_list(input, 256, FireworkExplosion::decode)?,
        })
    }
}

// ---- banner patterns -----------------------------------------------------------------------------------------------

/// `BannerPattern` (data-driven registry entry).
#[derive(Clone, Debug, PartialEq)]
pub struct BannerPattern {
    pub asset_id: Identifier,
    pub translation_key: String,
}

impl BannerPattern {
    pub fn direct_codec() -> Codec<BannerPattern> {
        record!(BannerPattern {
            asset_id: IDENTIFIER.field_of("asset_id"),
            translation_key: STRING.field_of("translation_key")
        })
        .codec()
    }
    /// `BannerPattern.CODEC`.
    pub fn codec() -> Codec<Holder<BannerPattern>> {
        holder::holder_file_codec(BANNER_PATTERN, &Self::direct_codec(), true)
    }
    pub fn encode_holder(h: &Holder<BannerPattern>, out: &mut Vec<u8>) {
        holder::encode_holder(BANNER_PATTERN, h, out, |p, o| {
            p.asset_id.encode(o);
            write_utf(o, &p.translation_key);
        })
    }
    pub fn decode_holder(input: &mut &[u8]) -> DecodeResult<Holder<BannerPattern>> {
        holder::decode_holder(BANNER_PATTERN, input, |i| {
            Ok(BannerPattern {
                asset_id: Identifier::decode(i)?,
                translation_key: read_utf(i, MAX_STRING_LENGTH)?,
            })
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BannerPatternLayer {
    pub pattern: Holder<BannerPattern>,
    pub color: DyeColor,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BannerPatternLayers {
    pub layers: Vec<BannerPatternLayer>,
}

impl BannerPatternLayers {
    pub fn codec() -> Codec<BannerPatternLayers> {
        record!(BannerPatternLayer {
            pattern: BannerPattern::codec().field_of("pattern"),
            color: DyeColor::codec().field_of("color")
        })
        .codec()
        .list_of()
        .xmap(
            |layers| BannerPatternLayers { layers },
            |l: &BannerPatternLayers| l.layers.clone(),
        )
    }
}

impl Encode for BannerPatternLayers {
    fn encode(&self, out: &mut Vec<u8>) {
        write_list(out, &self.layers, |l, o| {
            BannerPattern::encode_holder(&l.pattern, o);
            l.color.encode(o);
        });
    }
}
impl Decode for BannerPatternLayers {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_list(input, i32::MAX as usize, |i| {
            Ok(BannerPatternLayer {
                pattern: BannerPattern::decode_holder(i)?,
                color: DyeColor::decode(i)?,
            })
        })
        .map(|layers| BannerPatternLayers { layers })
    }
}

// ---- pot decorations -----------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PotDecorations {
    pub back: Option<ItemStackTemplate>,
    pub left: Option<ItemStackTemplate>,
    pub right: Option<ItemStackTemplate>,
    pub front: Option<ItemStackTemplate>,
}

impl PotDecorations {
    pub fn codec() -> Codec<PotDecorations> {
        let t = ItemStackTemplate::codec();
        record!(PotDecorations {
            back: t.optional_field_of("back"),
            left: t.optional_field_of("left"),
            right: t.optional_field_of("right"),
            front: t.optional_field_of("front"),
        })
        .codec()
    }
}

impl Encode for PotDecorations {
    fn encode(&self, out: &mut Vec<u8>) {
        for side in [&self.back, &self.left, &self.right, &self.front] {
            write_optional(out, side, |t, o| t.encode(o));
        }
    }
}
impl Decode for PotDecorations {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(PotDecorations {
            back: read_optional(input, ItemStackTemplate::decode)?,
            left: read_optional(input, ItemStackTemplate::decode)?,
            right: read_optional(input, ItemStackTemplate::decode)?,
            front: read_optional(input, ItemStackTemplate::decode)?,
        })
    }
}

/// `DecoratedPotPattern` (only ever referenced by id).
pub type DecoratedPotPattern = NoDirect;

/// `DecoratedPotPatterns.CODEC`.
pub fn decorated_pot_pattern_codec() -> Codec<Holder<DecoratedPotPattern>> {
    holder::holder_fixed_codec(DECORATED_POT_PATTERN)
}

// ---- sign text -----------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SignText {
    pub messages: Vec<Component>,
    pub filtered_messages: Vec<Component>,
    pub color: DyeColor,
    pub has_glowing_text: bool,
}

impl SignText {
    pub const LINES: usize = 4;

    pub fn codec() -> Codec<SignText> {
        struct Raw {
            messages: Vec<Component>,
            filtered: Option<Vec<Component>>,
            color: DyeColor,
            has_glowing_text: bool,
        }
        let lines = component_codec().list_of_range(4, 4);
        record!(Raw {
            messages: lines.field_of("messages"),
            filtered: lines.lenient_optional_field_of("filtered_messages"),
            color: optional_always_present_field_of(&DyeColor::codec(), "color", DyeColor::Black),
            has_glowing_text: optional_always_present_field_of(&BOOL, "has_glowing_text", false),
        })
        .codec()
        .xmap(
            |r: Raw| SignText {
                filtered_messages: r.filtered.unwrap_or_else(|| r.messages.clone()),
                messages: r.messages,
                color: r.color,
                has_glowing_text: r.has_glowing_text,
            },
            |s: &SignText| Raw {
                messages: s.messages.clone(),
                filtered: if s.filtered_messages == s.messages {
                    None
                } else {
                    Some(s.filtered_messages.clone())
                },
                color: s.color,
                has_glowing_text: s.has_glowing_text,
            },
        )
        .validate(|s| {
            if s.filtered_messages.len() != s.messages.len() {
                DataResult::error_str("Filtered and raw line counts are not equal")
            } else {
                DataResult::success(s)
            }
        })
    }
}

fn write_lines(lines: &[Component], out: &mut Vec<u8>) {
    assert_eq!(
        lines.len(),
        4,
        "Invalid list size, expected 4, but got {}",
        lines.len()
    );
    for l in lines {
        encode_component(l, out);
    }
}
fn read_lines(input: &mut &[u8]) -> DecodeResult<Vec<Component>> {
    (0..4).map(|_| decode_component(input)).collect()
}

impl Encode for SignText {
    fn encode(&self, out: &mut Vec<u8>) {
        write_lines(&self.messages, out);
        write_optional(
            out,
            &if self.filtered_messages == self.messages {
                None
            } else {
                Some(self.filtered_messages.clone())
            },
            |l, o| write_lines(l, o),
        );
        self.color.encode(out);
        self.has_glowing_text.encode(out);
    }
}
impl Decode for SignText {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let messages = read_lines(input)?;
        let filtered = read_optional(input, read_lines)?;
        Ok(SignText {
            filtered_messages: filtered.unwrap_or_else(|| messages.clone()),
            messages,
            color: DyeColor::decode(input)?,
            has_glowing_text: bool::decode(input)?,
        })
    }
}

// ---- instrument / jukebox song -------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Instrument {
    pub sound_event: Holder<SoundEvent>,
    pub use_duration: f32,
    pub range: f32,
    pub durability_damage: i32,
    pub description: Component,
}

impl Instrument {
    pub fn direct_codec() -> Codec<Instrument> {
        record!(Instrument {
            sound_event: SoundEvent::holder_codec().field_of("sound_event"),
            use_duration: NON_NEGATIVE_FLOAT.field_of("use_duration"),
            range: POSITIVE_FLOAT.field_of("range"),
            durability_damage: NON_NEGATIVE_INT.optional_field_of_default("durability_damage", 0),
            description: component_codec().field_of("description"),
        })
        .codec()
    }
    /// `Instrument.CODEC`.
    pub fn codec() -> Codec<Holder<Instrument>> {
        holder::holder_file_codec(INSTRUMENT_REG, &Self::direct_codec(), true)
    }
    fn encode_direct(&self, out: &mut Vec<u8>) {
        SoundEvent::encode_holder(&self.sound_event, out);
        self.use_duration.encode(out);
        self.range.encode(out);
        write_varint(out, self.durability_damage);
        encode_component(&self.description, out);
    }
    fn decode_direct(input: &mut &[u8]) -> DecodeResult<Instrument> {
        Ok(Instrument {
            sound_event: SoundEvent::decode_holder(input)?,
            use_duration: f32::decode(input)?,
            range: f32::decode(input)?,
            durability_damage: read_varint(input)?,
            description: decode_component(input)?,
        })
    }
    pub fn encode_holder(h: &Holder<Instrument>, out: &mut Vec<u8>) {
        holder::encode_holder(INSTRUMENT_REG, h, out, |v, o| v.encode_direct(o))
    }
    pub fn decode_holder(input: &mut &[u8]) -> DecodeResult<Holder<Instrument>> {
        holder::decode_holder(INSTRUMENT_REG, input, Instrument::decode_direct)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstrumentComponent {
    pub instrument: Holder<Instrument>,
}

impl InstrumentComponent {
    pub fn codec() -> Codec<InstrumentComponent> {
        Instrument::codec().xmap(
            |instrument| InstrumentComponent { instrument },
            |c: &InstrumentComponent| c.instrument.clone(),
        )
    }
}

impl Encode for InstrumentComponent {
    fn encode(&self, out: &mut Vec<u8>) {
        Instrument::encode_holder(&self.instrument, out)
    }
}
impl Decode for InstrumentComponent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Instrument::decode_holder(input).map(|instrument| InstrumentComponent { instrument })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JukeboxSong {
    pub sound_event: Holder<SoundEvent>,
    pub description: Component,
    pub length_in_seconds: f32,
    pub comparator_output: i32,
}

impl JukeboxSong {
    /// `JukeboxSong.CODEC` (`RegistryCodecs.holder(key)`: references only).
    pub fn codec() -> Codec<Holder<JukeboxSong>> {
        holder::holder_fixed_codec(JUKEBOX_SONG_REG)
    }
    fn encode_direct(&self, out: &mut Vec<u8>) {
        SoundEvent::encode_holder(&self.sound_event, out);
        encode_component(&self.description, out);
        self.length_in_seconds.encode(out);
        write_varint(out, self.comparator_output);
    }
    fn decode_direct(input: &mut &[u8]) -> DecodeResult<JukeboxSong> {
        Ok(JukeboxSong {
            sound_event: SoundEvent::decode_holder(input)?,
            description: decode_component(input)?,
            length_in_seconds: f32::decode(input)?,
            comparator_output: read_varint(input)?,
        })
    }
    pub fn encode_holder(h: &Holder<JukeboxSong>, out: &mut Vec<u8>) {
        holder::encode_holder(JUKEBOX_SONG_REG, h, out, |v, o| v.encode_direct(o))
    }
    pub fn decode_holder(input: &mut &[u8]) -> DecodeResult<Holder<JukeboxSong>> {
        holder::decode_holder(JUKEBOX_SONG_REG, input, JukeboxSong::decode_direct)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JukeboxPlayable {
    pub song: Holder<JukeboxSong>,
}

impl JukeboxPlayable {
    pub fn codec() -> Codec<JukeboxPlayable> {
        JukeboxSong::codec().xmap(
            |song| JukeboxPlayable { song },
            |c: &JukeboxPlayable| c.song.clone(),
        )
    }
}

impl Encode for JukeboxPlayable {
    fn encode(&self, out: &mut Vec<u8>) {
        JukeboxSong::encode_holder(&self.song, out)
    }
}
impl Decode for JukeboxPlayable {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        JukeboxSong::decode_holder(input).map(|song| JukeboxPlayable { song })
    }
}

// ---- trims ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct TrimMaterial {
    pub palette_id: Identifier,
    pub description: Component,
}

impl TrimMaterial {
    pub fn direct_codec() -> Codec<TrimMaterial> {
        record!(TrimMaterial {
            palette_id: IDENTIFIER.field_of("palette_id"),
            description: component_codec().field_of("description")
        })
        .codec()
    }
    /// `TrimMaterial.CODEC`.
    pub fn codec() -> Codec<Holder<TrimMaterial>> {
        holder::holder_file_codec(TRIM_MATERIAL_REG, &Self::direct_codec(), true)
    }
    pub fn encode_holder(h: &Holder<TrimMaterial>, out: &mut Vec<u8>) {
        holder::encode_holder(TRIM_MATERIAL_REG, h, out, |v, o| {
            v.palette_id.encode(o);
            encode_component(&v.description, o);
        })
    }
    pub fn decode_holder(input: &mut &[u8]) -> DecodeResult<Holder<TrimMaterial>> {
        holder::decode_holder(TRIM_MATERIAL_REG, input, |i| {
            Ok(TrimMaterial {
                palette_id: Identifier::decode(i)?,
                description: decode_component(i)?,
            })
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrimPattern {
    pub asset_id: Identifier,
    pub description: Component,
    pub decal: bool,
}

impl TrimPattern {
    pub fn direct_codec() -> Codec<TrimPattern> {
        record!(TrimPattern {
            asset_id: IDENTIFIER.field_of("asset_id"),
            description: component_codec().field_of("description"),
            decal: optional_always_present_field_of(&BOOL, "decal", false),
        })
        .codec()
    }
    /// `TrimPattern.CODEC`.
    pub fn codec() -> Codec<Holder<TrimPattern>> {
        holder::holder_file_codec(TRIM_PATTERN_REG, &Self::direct_codec(), true)
    }
    pub fn encode_holder(h: &Holder<TrimPattern>, out: &mut Vec<u8>) {
        holder::encode_holder(TRIM_PATTERN_REG, h, out, |v, o| {
            v.asset_id.encode(o);
            encode_component(&v.description, o);
            v.decal.encode(o);
        })
    }
    pub fn decode_holder(input: &mut &[u8]) -> DecodeResult<Holder<TrimPattern>> {
        holder::decode_holder(TRIM_PATTERN_REG, input, |i| {
            Ok(TrimPattern {
                asset_id: Identifier::decode(i)?,
                description: decode_component(i)?,
                decal: bool::decode(i)?,
            })
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArmorTrim {
    pub material: Holder<TrimMaterial>,
    pub pattern: Holder<TrimPattern>,
}

impl ArmorTrim {
    pub fn codec() -> Codec<ArmorTrim> {
        record!(ArmorTrim {
            material: TrimMaterial::codec().field_of("material"),
            pattern: TrimPattern::codec().field_of("pattern")
        })
        .codec()
    }
}

impl Encode for ArmorTrim {
    fn encode(&self, out: &mut Vec<u8>) {
        TrimMaterial::encode_holder(&self.material, out);
        TrimPattern::encode_holder(&self.pattern, out);
    }
}
impl Decode for ArmorTrim {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(ArmorTrim {
            material: TrimMaterial::decode_holder(input)?,
            pattern: TrimPattern::decode_holder(input)?,
        })
    }
}

// ---- definitions ---------------------------------------------------------------------------------------------------

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(
        ComponentDef::builder(WRITABLE_BOOK_CONTENT)
            .persistent(WritableBookContent::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(WRITTEN_BOOK_CONTENT)
            .persistent(WrittenBookContent::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(FIREWORK_EXPLOSION)
            .persistent(FireworkExplosion::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(FIREWORKS)
            .persistent(Fireworks::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(BANNER_PATTERNS)
            .persistent(BannerPatternLayers::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(POT_DECORATIONS)
            .persistent(PotDecorations::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(SIGN_TEXT_FRONT)
            .persistent(SignText::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(SIGN_TEXT_BACK)
            .persistent(SignText::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(INSTRUMENT)
            .persistent(InstrumentComponent::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(JUKEBOX_PLAYABLE)
            .persistent(JukeboxPlayable::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(TRIM)
            .persistent(ArmorTrim::codec())
            .network_synchronized()
            .build(),
    );
    out.push(
        ComponentDef::builder(PROVIDES_TRIM_MATERIAL)
            .persistent(TrimMaterial::codec())
            .network_with(TrimMaterial::encode_holder, TrimMaterial::decode_holder)
            .build(),
    );
    out.push(
        ComponentDef::builder(PROVIDES_BANNER_PATTERNS)
            .persistent(holder::holder_set_codec(
                BANNER_PATTERN,
                &holder::holder_fixed_codec::<BannerPattern>(BANNER_PATTERN),
                false,
            ))
            .network_with(
                |s, out| holder::encode_holder_set(BANNER_PATTERN, s, out),
                |i| holder::decode_holder_set(BANNER_PATTERN, i),
            )
            .build(),
    );
    out.push(
        ComponentDef::builder(PROVIDES_POTTERY_PATTERN)
            .persistent(decorated_pot_pattern_codec())
            .network_with(
                |h, out| holder::encode_holder_registry(DECORATED_POT_PATTERN, h, out),
                |i| holder::decode_holder_registry(DECORATED_POT_PATTERN, i),
            )
            .build(),
    );
}
