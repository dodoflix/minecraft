//! `ComponentSerialization.CODEC`, generic over the two ops (JSON / NBT), plus `Style`, `ClickEvent`,
//! `HoverEvent`, and the contents codecs.
//!
//! Decoding follows DFU semantics exactly where they are observable: `Either` alternatives are tried
//! in order, `optionalFieldOf` fields that are present but invalid make the whole record fail,
//! `lenientOptionalFieldOf` fields silently fall back, contents without a `type` field are matched
//! "fuzzily" in the fixed order text, translatable, keybind, score, selector, nbt, object.

use crate::component::*;
use crate::events::*;
use crate::java_util::{java_uuid_from_string, validate_chat_string, validate_untrusted_uri};
use crate::ops::{MapView, Ops, map_get};
use crate::style::{FontDescription, Style};
use crate::text_color::TextColor;
use mc_core::ident::Identifier;
use mc_nbt::Tag;
use std::fmt;
use std::sync::RwLock;

/// A `DataResult.error` message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodecError(pub String);

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CodecError {}

impl From<String> for CodecError {
    fn from(s: String) -> Self {
        CodecError(s)
    }
}

pub(crate) type R<T> = Result<T, CodecError>;

/// A `DataResult.error` that may carry a partial value (`DataResult.Error.partialValue`); `Ok` is a success.
#[derive(Clone, Debug, PartialEq)]
pub struct Partial<T> {
    pub error: CodecError,
    pub value: Option<T>,
}

/// `DataResult<T>`: a success, or an error with an optional partial value.
pub type P<T> = Result<T, Box<Partial<T>>>;

impl<T> From<CodecError> for Box<Partial<T>> {
    fn from(error: CodecError) -> Self {
        Box::new(Partial { error, value: None })
    }
}

impl<T> From<Box<Partial<T>>> for CodecError {
    fn from(p: Box<Partial<T>>) -> Self {
        p.error
    }
}

/// `DataResult.map`.
fn map_p<T, U>(r: P<T>, f: impl FnOnce(T) -> U) -> P<U> {
    match r {
        Ok(v) => Ok(f(v)),
        Err(p) => Err(Box::new(Partial {
            error: p.error,
            value: p.value.map(f),
        })),
    }
}

/// Results the field helpers accept: a plain [`R`] (no partial value) or a [`P`].
pub(crate) trait IntoP {
    type T;
    fn into_p(self) -> P<Self::T>;
}

impl<T> IntoP for R<T> {
    type T = T;
    fn into_p(self) -> P<T> {
        self.map_err(Into::into)
    }
}

impl<T> IntoP for P<T> {
    type T = T;
    fn into_p(self) -> P<T> {
        self
    }
}

/// `DataResult.ap` over the fields of a record: errors are joined, and the result keeps a partial value only
/// when every field has a value or a partial value.
#[derive(Default)]
struct Record {
    error: Option<String>,
    lost: bool,
}

impl Record {
    fn take<X: IntoP>(&mut self, r: X) -> Option<X::T> {
        match r.into_p() {
            Ok(v) => Some(v),
            Err(p) => {
                self.error = Some(match self.error.take() {
                    Some(e) => format!("{e}; {}", p.error.0),
                    None => p.error.0,
                });
                self.lost |= p.value.is_none();
                p.value
            }
        }
    }

    /// `make` builds the record from the taken fields; it returns `None` when a field has no value.
    fn finish<T>(self, make: impl FnOnce() -> Option<T>) -> P<T> {
        match self.error {
            None => Ok(make().expect("every field of a successful record has a value")),
            Some(error) => Err(Box::new(Partial {
                error: CodecError(error),
                value: if self.lost { None } else { make() },
            })),
        }
    }
}

/// `ListCodec.decode`: elements that fail without a partial value are dropped, the others (including partial
/// ones) are kept, and any failure makes the whole list a partial result.
fn decode_list_partial<O: Ops, E>(v: &O, f: impl Fn(&O) -> P<E>) -> P<Vec<E>> {
    let items = v
        .get_list()
        .ok_or_else(|| CodecError("Not a list".into()))?;
    let mut elements = Vec::with_capacity(items.len());
    let mut error: Option<String> = None;
    for item in &items {
        let value = match f(item.as_ref()) {
            Ok(e) => Some(e),
            Err(p) => {
                error = Some(match error.take() {
                    Some(e) => format!("{e}; {}", p.error.0),
                    None => p.error.0,
                });
                p.value
            }
        };
        elements.extend(value);
    }
    match error {
        None => Ok(elements),
        Some(error) => Err(Box::new(Partial {
            error: CodecError(error),
            value: Some(elements),
        })),
    }
}

pub(crate) fn err<T>(msg: impl Into<String>) -> R<T> {
    Err(CodecError(msg.into()))
}

/// Registry/command-dependent validation the codecs perform in vanilla but which needs data owned
/// by other crates (entity selector parser, NBT path parser, coordinate parser, entity type and
/// item registries). Install with [`set_validators`]; the default accepts everything.
#[derive(Clone, Copy)]
pub struct Validators {
    /// `EntitySelector.COMPILABLE_CODEC` (`selector` contents, `entity` data source, `score` name).
    pub entity_selector: fn(&str) -> Result<(), String>,
    /// `NbtContents.NBT_PATH_CODEC`.
    pub nbt_path: fn(&str) -> Result<(), String>,
    /// `BlockDataSource.BLOCK_POS_CODEC`.
    pub block_coordinates: fn(&str) -> Result<(), String>,
    /// Entity type registry lookup (`show_entity`).
    pub entity_type_exists: fn(&Identifier) -> bool,
    /// Item registry lookup (`show_item`).
    pub item_exists: fn(&Identifier) -> bool,
}

fn accept_str(_: &str) -> Result<(), String> {
    Ok(())
}
fn accept_id(_: &Identifier) -> bool {
    true
}

impl Default for Validators {
    fn default() -> Self {
        Validators {
            entity_selector: accept_str,
            nbt_path: accept_str,
            block_coordinates: accept_str,
            entity_type_exists: accept_id,
            item_exists: accept_id,
        }
    }
}

static VALIDATORS: RwLock<Option<Validators>> = RwLock::new(None);

/// Installs the process-wide validators used while decoding components.
pub fn set_validators(v: Validators) {
    *VALIDATORS.write().unwrap_or_else(|e| e.into_inner()) = Some(v);
}

fn validators() -> Validators {
    (*VALIDATORS.read().unwrap_or_else(|e| e.into_inner())).unwrap_or_default()
}

/// Which typed payload a hook normalizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadKind {
    /// `show_item` `components` (`DataComponentPatch.CODEC`).
    ItemComponents,
    /// `show_dialog` `dialog` (`Dialog.CODEC`: a registry id string or an inline dialog compound).
    Dialog,
}

/// Normalization of payloads whose vanilla codecs need the data component / dialog registries (owned by
/// `mc-protocol` and above): the value is decoded with the typed codec of the *input* ops and re-encoded as NBT, so
/// numbers get their canonical tag types (a JSON `damage:3` becomes an int, not a byte). `to_json` re-encodes the stored
/// NBT through the typed codec with JsonOps. The default is the identity (opaque NBT), installed by
/// `mc_protocol::components::install_text_hooks`.
#[derive(Clone, Copy)]
pub struct PayloadHooks {
    pub from_json: fn(PayloadKind, &serde_json::Value) -> Result<Tag, String>,
    pub from_nbt: fn(PayloadKind, &Tag) -> Result<Tag, String>,
    pub to_json: fn(PayloadKind, &Tag) -> Result<serde_json::Value, String>,
}

impl Default for PayloadHooks {
    fn default() -> Self {
        PayloadHooks {
            from_json: |_, v| Ok(<serde_json::Value as Ops>::to_tag(v)),
            from_nbt: |_, t| Ok(t.clone()),
            to_json: |_, t| Ok(<serde_json::Value as Ops>::from_tag(t)),
        }
    }
}

static PAYLOAD_HOOKS: RwLock<Option<PayloadHooks>> = RwLock::new(None);

/// Installs the process-wide payload normalizers.
pub fn set_payload_hooks(h: PayloadHooks) {
    *PAYLOAD_HOOKS.write().unwrap_or_else(|e| e.into_inner()) = Some(h);
}

pub(crate) fn payload_hooks() -> PayloadHooks {
    (*PAYLOAD_HOOKS.read().unwrap_or_else(|e| e.into_inner())).unwrap_or_default()
}

// ------------------------------------------------------------------------------------------
// field helpers
// ------------------------------------------------------------------------------------------

pub(crate) fn req<'a, O: Ops>(map: &MapView<'a, O>, key: &str) -> R<&'a O> {
    map_get(map, key).ok_or_else(|| CodecError(format!("No key {key} in MapLike")))
}

/// `optionalFieldOf`: absent -> `None`; present but invalid -> error.
pub(crate) fn opt<'a, O: Ops, T>(
    map: &MapView<'a, O>,
    key: &str,
    f: impl FnOnce(&'a O) -> R<T>,
) -> R<Option<T>> {
    match map_get(map, key) {
        None => Ok(None),
        Some(v) => f(v).map(Some),
    }
}

/// `OptionalFieldCodec.decode`: like [`opt`], but a present, invalid field is an error whose partial value is the
/// field's partial value (`Optional.empty` when it has none).
fn opt_p<'a, O: Ops, X: IntoP>(
    map: &MapView<'a, O>,
    key: &str,
    f: impl FnOnce(&'a O) -> X,
) -> P<Option<X::T>> {
    match map_get(map, key) {
        None => Ok(None),
        Some(v) => match f(v).into_p() {
            Ok(t) => Ok(Some(t)),
            Err(p) => Err(Box::new(Partial {
                error: p.error,
                value: Some(p.value),
            })),
        },
    }
}

/// `lenientOptionalFieldOf`: absent or invalid -> `None`.
fn lenient<'a, O: Ops, T>(
    map: &MapView<'a, O>,
    key: &str,
    f: impl FnOnce(&'a O) -> R<T>,
) -> Option<T> {
    map_get(map, key).and_then(|v| f(v).ok())
}

pub(crate) fn string<O: Ops>(v: &O) -> R<String> {
    v.get_string()
        .ok_or_else(|| CodecError("Not a string".into()))
}
fn boolean<O: Ops>(v: &O) -> R<bool> {
    v.get_bool()
        .ok_or_else(|| CodecError("Not a boolean".into()))
}
fn int<O: Ops>(v: &O) -> R<i32> {
    v.get_int().ok_or_else(|| CodecError("Not a number".into()))
}
pub(crate) fn identifier<O: Ops>(v: &O) -> R<Identifier> {
    Identifier::read(&string(v)?).map_err(CodecError)
}
pub(crate) fn map_of<O: Ops>(v: &O) -> R<MapView<'_, O>> {
    v.get_map().ok_or_else(|| CodecError("Not a map".into()))
}

// ------------------------------------------------------------------------------------------
// Component
// ------------------------------------------------------------------------------------------

/// `ComponentSerialization.CODEC.parse`.
pub fn decode_component<O: Ops>(v: &O) -> R<Component> {
    Ok(decode_component_partial(v)?)
}

/// `ComponentSerialization.CODEC.parse(..).resultOrPartial(..)`: the component, or the error with the partial component.
pub fn decode_component_partial<O: Ops>(v: &O) -> P<Component> {
    // Either<Either<String, nonEmptyList<Component>>, fullCodec>
    if let Some(s) = v.get_string() {
        return Ok(Component::literal(s));
    }
    let list = v.get_list().is_some().then(|| decode_component_list(v));
    if let Some(Ok(list)) = list {
        return Ok(create_from_list(list));
    }
    decode_full(v).map_err(|full| match list {
        Some(Err(p)) if p.value.is_some() => Box::new(Partial {
            error: p.error,
            value: p.value.map(create_from_list),
        }),
        _ => full,
    })
}

/// `ExtraCodecs.nonEmptyList(topSerializer.listOf())`.
fn decode_component_list<O: Ops>(v: &O) -> P<Vec<Component>> {
    match decode_list_partial(v, decode_component_partial) {
        Ok(list) if list.is_empty() => Err(CodecError("List must have contents".into()).into()),
        Err(p) if p.value.as_ref().is_some_and(Vec::is_empty) => Err(p.error.into()),
        r => r,
    }
}

/// `createFromList`: the first component with the rest appended as siblings.
fn create_from_list(list: Vec<Component>) -> Component {
    let mut it = list.into_iter();
    let mut result = it.next().expect("non-empty list");
    for c in it {
        result.push_sibling(c);
    }
    result
}

fn decode_full<O: Ops>(v: &O) -> P<Component> {
    let map = v.get_map().ok_or_else(|| CodecError("Not a map".into()))?;
    let mut record = Record::default();
    let contents = record.take(decode_contents(&map));
    let siblings = record
        .take(opt_p(&map, "extra", decode_component_list))
        .flatten();
    let style = record.take(decode_style(&map));
    record.finish(|| {
        Some(Component {
            contents: contents?,
            siblings: siblings.unwrap_or_default(),
            style: style?,
        })
    })
}

/// `ComponentSerialization.CODEC.encodeStart`.
pub fn encode_component<O: Ops>(c: &Component) -> R<O> {
    if let Some(text) = c.try_collapse_to_string() {
        return Ok(O::create_string(text));
    }
    let mut fields = Vec::new();
    encode_contents::<O>(&c.contents, &mut fields)?;
    if !c.siblings.is_empty() {
        let items = c
            .siblings
            .iter()
            .map(encode_component::<O>)
            .collect::<R<Vec<_>>>()?;
        fields.push(("extra".to_owned(), O::create_list(items)));
    }
    encode_style_fields::<O>(&c.style, &mut fields)?;
    Ok(O::create_map(fields))
}

pub(crate) type Fields<O> = Vec<(String, O)>;

fn push<O>(fields: &mut Fields<O>, key: &str, v: O) {
    fields.push((key.to_owned(), v));
}

// ------------------------------------------------------------------------------------------
// contents
// ------------------------------------------------------------------------------------------

const CONTENT_TYPES: [&str; 7] = [
    "text",
    "translatable",
    "keybind",
    "score",
    "selector",
    "nbt",
    "object",
];

fn decode_contents_of_type<O: Ops>(ty: &str, map: &MapView<'_, O>) -> P<ComponentContents> {
    Ok(match ty {
        "text" => ComponentContents::Literal(string(req(map, "text")?)?),
        "translatable" => return decode_translatable(map),
        "keybind" => ComponentContents::Keybind(string(req(map, "keybind")?)?),
        "score" => decode_score(map)?,
        "selector" => return decode_selector(map),
        "nbt" => decode_nbt(map)?,
        "object" => return decode_object(map),
        _ => return Err(CodecError(format!("Unknown element id: {ty}")).into()),
    })
}

/// `createLegacyComponentMatcher(types, ComponentContents::codec, "type")` decode direction. Without a `type` field
/// the fuzzy matcher only accepts a full success, so a partial result is dropped there.
fn decode_contents<O: Ops>(map: &MapView<'_, O>) -> P<ComponentContents> {
    if let Some(ty) = map_get(map, "type") {
        return decode_contents_of_type(&string(ty)?, map);
    }
    CONTENT_TYPES
        .into_iter()
        .find_map(|ty| decode_contents_of_type(ty, map).ok())
        .ok_or_else(|| CodecError("No matching codec found".into()).into())
}

fn decode_translatable<O: Ops>(map: &MapView<'_, O>) -> P<ComponentContents> {
    let key = string(req(map, "translate")?)?;
    let fallback = lenient(map, "fallback", string);
    let mut record = Record::default();
    let args = record
        .take(opt_p(map, "with", |w| decode_list_partial(w, decode_arg)))
        .flatten()
        .unwrap_or_default();
    record.finish(|| {
        Some(ComponentContents::Translatable(TranslatableContents {
            key,
            fallback,
            args,
        }))
    })
}

/// `TranslatableContents.ARG_CODEC`.
fn decode_arg<O: Ops>(v: &O) -> P<TranslatableArg> {
    if let Some(p) = v.get_primitive_arg() {
        return Ok(p);
    }
    map_p(decode_component_partial(v), |c| {
        match c.try_collapse_to_string() {
            Some(s) => TranslatableArg::String(s.to_owned()),
            None => TranslatableArg::Component(c),
        }
    })
}

fn decode_selector_string(s: String) -> R<String> {
    (validators().entity_selector)(&s).map_err(CodecError)?;
    Ok(s)
}

fn decode_score<O: Ops>(map: &MapView<'_, O>) -> R<ComponentContents> {
    let inner = map_of(req(map, "score")?)?;
    let name = string(req(&inner, "name")?)?;
    let objective = string(req(&inner, "objective")?)?;
    // Either<CompilableString<EntitySelector>, String>: a valid selector wins.
    let name = match (validators().entity_selector)(&name) {
        Ok(()) => ScoreName::Selector(name),
        Err(_) => ScoreName::Name(name),
    };
    Ok(ComponentContents::Score(ScoreContents { name, objective }))
}

fn decode_selector<O: Ops>(map: &MapView<'_, O>) -> P<ComponentContents> {
    let selector = decode_selector_string(string(req(map, "selector")?)?)?;
    let mut record = Record::default();
    let separator = record
        .take(opt_p(map, "separator", decode_component_partial))
        .flatten()
        .map(Box::new);
    record.finish(|| {
        Some(ComponentContents::Selector(SelectorContents {
            selector,
            separator,
        }))
    })
}

fn decode_data_source_of_type<O: Ops>(ty: &str, map: &MapView<'_, O>) -> R<DataSource> {
    match ty {
        "entity" => Ok(DataSource::Entity(decode_selector_string(string(req(
            map, "entity",
        )?)?)?)),
        "block" => {
            let s = string(req(map, "block")?)?;
            (validators().block_coordinates)(&s).map_err(CodecError)?;
            Ok(DataSource::Block(s))
        }
        "storage" => Ok(DataSource::Storage(identifier(req(map, "storage")?)?)),
        _ => err(format!("Unknown element id: {ty}")),
    }
}

fn decode_data_source<O: Ops>(map: &MapView<'_, O>) -> R<DataSource> {
    if let Some(ty) = map_get(map, "source") {
        return decode_data_source_of_type(&string(ty)?, map);
    }
    for ty in ["entity", "block", "storage"] {
        if let Ok(d) = decode_data_source_of_type(ty, map) {
            return Ok(d);
        }
    }
    err("No matching codec found")
}

fn decode_nbt<O: Ops>(map: &MapView<'_, O>) -> R<ComponentContents> {
    let nbt_path = string(req(map, "nbt")?)?;
    (validators().nbt_path)(&nbt_path).map_err(CodecError)?;
    let interpreting = lenient(map, "interpret", boolean).unwrap_or(false);
    let plain = lenient(map, "plain", boolean).unwrap_or(false);
    let separator = lenient(map, "separator", |v| decode_component(v)).map(Box::new);
    let data_source = decode_data_source(map)?;
    if interpreting && plain {
        return err("'interpret' and 'plain' flags can't be both on");
    }
    Ok(ComponentContents::Nbt(NbtContents {
        nbt_path,
        interpreting,
        plain,
        separator,
        data_source,
    }))
}

fn decode_object_info_of_type<O: Ops>(ty: &str, map: &MapView<'_, O>) -> P<ObjectInfo> {
    let mut record = Record::default();
    match ty {
        "atlas" => {
            let atlas = record
                .take(opt_p(map, "atlas", |v| identifier(v)))
                .flatten();
            let sprite = identifier(req(map, "sprite")?)?;
            record.finish(|| {
                Some(ObjectInfo::AtlasSprite {
                    atlas: atlas.unwrap_or_else(ObjectInfo::default_atlas),
                    sprite,
                })
            })
        }
        "player" => {
            let player = Box::new(crate::profile::decode_profile(req(map, "player")?)?);
            let hat = record.take(opt_p(map, "hat", boolean)).flatten();
            record.finish(|| {
                Some(ObjectInfo::PlayerSprite {
                    player,
                    hat: hat.unwrap_or(true),
                })
            })
        }
        _ => Err(CodecError(format!("Unknown element id: {ty}")).into()),
    }
}

fn decode_object<O: Ops>(map: &MapView<'_, O>) -> P<ComponentContents> {
    let mut record = Record::default();
    let contents = if let Some(ty) = map_get(map, "object") {
        record.take(decode_object_info_of_type(&string(ty)?, map))
    } else {
        let found = ["atlas", "player"]
            .into_iter()
            .find_map(|ty| decode_object_info_of_type(ty, map).ok());
        Some(found.ok_or_else(|| CodecError("No matching codec found".into()))?)
    };
    let fallback = record
        .take(opt_p(map, "fallback", decode_component_partial))
        .flatten()
        .map(Box::new);
    record.finish(|| {
        Some(ComponentContents::Object(ObjectContents {
            contents: contents?,
            fallback,
        }))
    })
}

fn encode_contents<O: Ops>(c: &ComponentContents, f: &mut Fields<O>) -> R<()> {
    match c {
        ComponentContents::Literal(t) => push(f, "text", O::create_string(t)),
        ComponentContents::Translatable(t) => {
            push(f, "translate", O::create_string(&t.key));
            if let Some(fb) = &t.fallback {
                push(f, "fallback", O::create_string(fb));
            }
            if !t.args.is_empty() {
                let args = t.args.iter().map(encode_arg::<O>).collect::<R<Vec<_>>>()?;
                push(f, "with", O::create_list(args));
            }
        }
        ComponentContents::Keybind(k) => push(f, "keybind", O::create_string(k)),
        ComponentContents::Score(s) => {
            let name = match &s.name {
                ScoreName::Selector(n) | ScoreName::Name(n) => n,
            };
            push(
                f,
                "score",
                O::create_map(vec![
                    ("name".into(), O::create_string(name)),
                    ("objective".into(), O::create_string(&s.objective)),
                ]),
            );
        }
        ComponentContents::Selector(s) => {
            push(f, "selector", O::create_string(&s.selector));
            if let Some(sep) = &s.separator {
                push(f, "separator", encode_component::<O>(sep)?);
            }
        }
        ComponentContents::Nbt(n) => {
            if n.interpreting && n.plain {
                return err("'interpret' and 'plain' flags can't be both on");
            }
            push(f, "nbt", O::create_string(&n.nbt_path));
            if n.interpreting {
                push(f, "interpret", O::create_bool(true));
            }
            if n.plain {
                push(f, "plain", O::create_bool(true));
            }
            if let Some(sep) = &n.separator {
                push(f, "separator", encode_component::<O>(sep)?);
            }
            match &n.data_source {
                DataSource::Entity(s) => push(f, "entity", O::create_string(s)),
                DataSource::Block(s) => push(f, "block", O::create_string(s)),
                DataSource::Storage(id) => push(f, "storage", O::create_string(&id.to_string())),
            }
        }
        ComponentContents::Object(o) => {
            match &o.contents {
                ObjectInfo::AtlasSprite { atlas, sprite } => {
                    if *atlas != ObjectInfo::default_atlas() {
                        push(f, "atlas", O::create_string(&atlas.to_string()));
                    }
                    push(f, "sprite", O::create_string(&sprite.to_string()));
                }
                ObjectInfo::PlayerSprite { player, hat } => {
                    push(f, "player", crate::profile::encode_profile::<O>(player));
                    if !*hat {
                        push(f, "hat", O::create_bool(false));
                    }
                }
            }
            if let Some(fb) = &o.fallback {
                push(f, "fallback", encode_component::<O>(fb)?);
            }
        }
    }
    Ok(())
}

fn encode_arg<O: Ops>(arg: &TranslatableArg) -> R<O> {
    match arg {
        TranslatableArg::Component(c) => encode_component::<O>(c),
        other => O::create_primitive_arg(other)
            .ok_or_else(|| CodecError("Unencodable translation argument".into())),
    }
}

// ------------------------------------------------------------------------------------------
// Style
// ------------------------------------------------------------------------------------------

/// `ExtraCodecs.ARGB_COLOR_CODEC`: an int, or a 4 float list `[r, g, b, a]`.
fn decode_argb<O: Ops>(v: &O) -> R<i32> {
    if let Some(i) = v.get_int() {
        return Ok(i);
    }
    let items = v
        .get_list()
        .ok_or_else(|| CodecError("Not a list".into()))?;
    if items.len() != 4 {
        return err("Input is not a list of 4 elements");
    }
    let f: Vec<f32> = items
        .iter()
        .map(|i| {
            i.get_float()
                .ok_or_else(|| CodecError("Not a number".into()))
        })
        .collect::<R<_>>()?;
    let ch = |v: f32| (mc_floor(v * 255.0f32) & 0xFF) as u32;
    Ok(((ch(f[3]) << 24) | (ch(f[0]) << 16) | (ch(f[1]) << 8) | ch(f[2])) as i32)
}

/// `Mth.floor(float)`.
fn mc_floor(v: f32) -> i32 {
    let i = v as i32;
    if v < i as f32 { i.wrapping_sub(1) } else { i }
}

pub(crate) fn decode_style<O: Ops>(map: &MapView<'_, O>) -> P<Style> {
    let mut record = Record::default();
    let color = record
        .take(opt_p(map, "color", |v| {
            TextColor::parse_color(&string(v)?).map_err(CodecError)
        }))
        .flatten();
    let shadow_color = record
        .take(opt_p(map, "shadow_color", |v| decode_argb(v)))
        .flatten();
    let bold = record.take(opt_p(map, "bold", boolean)).flatten();
    let italic = record.take(opt_p(map, "italic", boolean)).flatten();
    let underlined = record.take(opt_p(map, "underlined", boolean)).flatten();
    let strikethrough = record.take(opt_p(map, "strikethrough", boolean)).flatten();
    let obfuscated = record.take(opt_p(map, "obfuscated", boolean)).flatten();
    let click_event = record
        .take(opt_p(map, "click_event", |v| decode_click_event(v)))
        .flatten();
    let hover_event = record
        .take(opt_p(map, "hover_event", |v| decode_hover_event(v)))
        .flatten();
    let insertion = record.take(opt_p(map, "insertion", string)).flatten();
    let font = record
        .take(opt_p(map, "font", |v| {
            identifier(v).map(FontDescription::Resource)
        }))
        .flatten();
    record.finish(|| {
        Some(Style::create(
            color,
            shadow_color,
            bold,
            italic,
            underlined,
            strikethrough,
            obfuscated,
            click_event,
            hover_event,
            insertion,
            font,
        ))
    })
}

/// `Style.Serializer.CODEC` decode (a map of style fields).
pub fn decode_style_value<O: Ops>(v: &O) -> R<Style> {
    Ok(decode_style(&map_of(v)?)?)
}

/// `Style.Serializer.CODEC` encode.
pub fn encode_style_value<O: Ops>(s: &Style) -> R<O> {
    let mut fields = Vec::new();
    encode_style_fields::<O>(s, &mut fields)?;
    Ok(O::create_map(fields))
}

pub(crate) fn encode_style_fields<O: Ops>(s: &Style, f: &mut Fields<O>) -> R<()> {
    if let Some(c) = s.color() {
        push(f, "color", O::create_string(&c.serialize()));
    }
    if let Some(c) = s.shadow_color() {
        push(f, "shadow_color", O::create_int(c));
    }
    for (name, v) in [
        ("bold", s.bold()),
        ("italic", s.italic()),
        ("underlined", s.underlined()),
        ("strikethrough", s.strikethrough()),
        ("obfuscated", s.obfuscated()),
    ] {
        if let Some(v) = v {
            push(f, name, O::create_bool(v));
        }
    }
    if let Some(c) = s.click_event() {
        push(f, "click_event", encode_click_event::<O>(c)?);
    }
    if let Some(h) = s.hover_event() {
        push(f, "hover_event", encode_hover_event::<O>(h)?);
    }
    if let Some(i) = s.insertion() {
        push(f, "insertion", O::create_string(i));
    }
    if let Some(font) = s.font_opt() {
        match font {
            FontDescription::Resource(id) => push(f, "font", O::create_string(&id.to_string())),
            other => return err(format!("Unsupported font description type: {other:?}")),
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------
// ClickEvent / HoverEvent
// ------------------------------------------------------------------------------------------

fn action_name<O: Ops>(map: &MapView<'_, O>) -> R<String> {
    string(req(map, "action")?)
}

pub(crate) fn decode_click_event<O: Ops>(v: &O) -> R<ClickEvent> {
    let map = map_of(v)?;
    let name = action_name(&map)?;
    let action = ClickAction::ALL
        .into_iter()
        .find(|a| a.serialized_name() == name)
        .ok_or_else(|| CodecError(format!("Unknown element name:{name}")))?;
    if !action.is_allowed_from_server() {
        return err(format!(
            "Click event type not allowed: {}",
            name.to_ascii_uppercase()
        ));
    }
    Ok(match action {
        ClickAction::OpenUrl => {
            let url = string(req(&map, "url")?)?;
            validate_untrusted_uri(&url).map_err(CodecError)?;
            ClickEvent::OpenUrl(url)
        }
        ClickAction::OpenFile => unreachable!("rejected above"),
        ClickAction::RunCommand => {
            ClickEvent::RunCommand(decode_chat_string(req(&map, "command")?)?)
        }
        ClickAction::SuggestCommand => {
            ClickEvent::SuggestCommand(decode_chat_string(req(&map, "command")?)?)
        }
        // `Dialog.CODEC` (a registry id or an inline dialog): validated and normalised by the installed `PayloadKind::Dialog` hook
        ClickAction::ShowDialog => ClickEvent::ShowDialog(
            req(&map, "dialog")?
                .normalize_payload(PayloadKind::Dialog)
                .map_err(CodecError)?,
        ),
        ClickAction::ChangePage => {
            let page = int(req(&map, "page")?)?;
            if page < 1 {
                return err(format!("Value must be positive: {page}"));
            }
            ClickEvent::ChangePage(page)
        }
        ClickAction::CopyToClipboard => ClickEvent::CopyToClipboard(string(req(&map, "value")?)?),
        ClickAction::Custom => {
            let id = identifier(req(&map, "id")?)?;
            let payload = opt(&map, "payload", |p| Ok::<_, CodecError>(p.to_tag()))?;
            ClickEvent::Custom { id, payload }
        }
    })
}

fn decode_chat_string<O: Ops>(v: &O) -> R<String> {
    let s = string(v)?;
    validate_chat_string(&s).map_err(CodecError)?;
    Ok(s)
}

pub(crate) fn encode_click_event<O: Ops>(c: &ClickEvent) -> R<O> {
    let mut f: Fields<O> = Vec::new();
    match c {
        ClickEvent::OpenUrl(u) => push(&mut f, "url", O::create_string(u)),
        ClickEvent::OpenFile(_) => return err("Click event type not allowed: OPEN_FILE"),
        ClickEvent::RunCommand(s) | ClickEvent::SuggestCommand(s) => {
            validate_chat_string(s).map_err(CodecError)?;
            push(&mut f, "command", O::create_string(s));
        }
        ClickEvent::ShowDialog(d) => push(
            &mut f,
            "dialog",
            O::payload_from_tag(PayloadKind::Dialog, d)?,
        ),
        ClickEvent::ChangePage(p) => {
            if *p < 1 {
                return err(format!("Value must be positive: {p}"));
            }
            push(&mut f, "page", O::create_int(*p));
        }
        ClickEvent::CopyToClipboard(s) => push(&mut f, "value", O::create_string(s)),
        ClickEvent::Custom { id, payload } => {
            push(&mut f, "id", O::create_string(&id.to_string()));
            if let Some(p) = payload {
                push(&mut f, "payload", O::from_tag(p));
            }
        }
    }
    push(
        &mut f,
        "action",
        O::create_string(c.action().serialized_name()),
    );
    Ok(O::create_map(f))
}

fn decode_uuid<O: Ops>(v: &O) -> R<u128> {
    // UUIDUtil.LENIENT_CODEC: int stream of 4, else a UUID string.
    if let Ok(u) = crate::profile::uuid_from_ints(v) {
        return Ok(u);
    }
    let s = string(v).map_err(|_| CodecError("Not a UUID".into()))?;
    java_uuid_from_string(&s).ok_or_else(|| CodecError(format!("Invalid UUID {s}")))
}

fn decode_hover_event<O: Ops>(v: &O) -> P<HoverEvent> {
    let map = map_of(v)?;
    let name = action_name(&map)?;
    match name.as_str() {
        "show_text" => map_p(decode_component_partial(req(&map, "value")?), |c| {
            HoverEvent::ShowText(Box::new(c))
        }),
        "show_item" => {
            let id = identifier(req(&map, "id")?)?;
            if !(validators().item_exists)(&id) {
                return Err(CodecError(format!(
                    "Unknown registry key in ResourceKey[minecraft:root / minecraft:item]: {id}"
                ))
                .into());
            }
            let mut record = Record::default();
            let count = record
                .take(opt_p(&map, "count", |v| match int(v)? {
                    c if (1..=99).contains(&c) => Ok(c),
                    c => err(format!("Value must be within range [1;99]: {c}")),
                }))
                .flatten()
                .unwrap_or(1);
            // shortcut: the component patch payload is taken to have no partial result, upgrade with DataComponentPatch.CODEC
            let components = record
                .take(opt_p(&map, "components", |c| {
                    if c.get_map().is_none() {
                        return err("Not a map");
                    }
                    c.normalize_payload(PayloadKind::ItemComponents)
                        .map_err(CodecError)
                }))
                .flatten()
                .filter(|t| !matches!(t, Tag::Compound(c) if c.is_empty()));
            record.finish(|| {
                Some(HoverEvent::ShowItem(ItemStackTemplate {
                    id,
                    count,
                    components,
                }))
            })
        }
        "show_entity" => {
            let entity_type = identifier(req(&map, "id")?)?;
            if !(validators().entity_type_exists)(&entity_type) {
                return Err(CodecError(format!(
                    "Unknown registry key in ResourceKey[minecraft:root / minecraft:entity_type]: {entity_type}"
                ))
                .into());
            }
            let uuid = decode_uuid(req(&map, "uuid")?)?;
            let mut record = Record::default();
            let name = record
                .take(opt_p(&map, "name", decode_component_partial))
                .flatten()
                .map(Box::new);
            record.finish(|| {
                Some(HoverEvent::ShowEntity(EntityTooltipInfo {
                    entity_type,
                    uuid,
                    name,
                }))
            })
        }
        _ => Err(CodecError(format!("Unknown element name:{name}")).into()),
    }
}

fn encode_hover_event<O: Ops>(h: &HoverEvent) -> R<O> {
    let mut f: Fields<O> = Vec::new();
    match h {
        HoverEvent::ShowText(c) => push(&mut f, "value", encode_component::<O>(c)?),
        HoverEvent::ShowItem(i) => {
            push(&mut f, "id", O::create_string(&i.id.to_string()));
            if i.count != 1 {
                if !(1..=99).contains(&i.count) {
                    return err(format!("Value must be within range [1;99]: {}", i.count));
                }
                push(&mut f, "count", O::create_int(i.count));
            }
            if let Some(c) = &i.components {
                push(
                    &mut f,
                    "components",
                    O::payload_from_tag(PayloadKind::ItemComponents, c)?,
                );
            }
        }
        HoverEvent::ShowEntity(e) => {
            push(&mut f, "id", O::create_string(&e.entity_type.to_string()));
            push(
                &mut f,
                "uuid",
                O::create_int_list(&crate::profile::uuid_to_ints(e.uuid)),
            );
            if let Some(n) = &e.name {
                push(&mut f, "name", encode_component::<O>(n)?);
            }
        }
    }
    push(
        &mut f,
        "action",
        O::create_string(h.action().serialized_name()),
    );
    Ok(O::create_map(f))
}
