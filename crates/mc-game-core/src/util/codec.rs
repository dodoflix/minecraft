//! Tiny DFU-on-`NbtOps` reading helpers: what `Codec.INT/LONG/BOOL/...`, `fieldOf` and `optionalFieldOf`
//! accept. Errors are plain strings like `DataResult.error`; the saved-data layer logs them like vanilla.

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, ListTag, Tag};

pub type DataResult<T> = Result<T, String>;

/// `DataResult.error(message, partialResult)`: a failed decode that may still carry a usable partial value
/// (`resultOrPartial` consumers — the saved-data reader — keep it and log the message).
#[derive(Debug)]
pub struct Partial<T> {
    pub value: Option<T>,
    pub error: String,
}

impl<T> From<String> for Partial<T> {
    fn from(error: String) -> Self {
        Partial { value: None, error }
    }
}

/// A decode result whose error may carry a partial value.
pub type DecodeResult<T> = Result<T, Partial<T>>;

/// `DataResult.map` over both the success and the partial value.
pub fn map_decoded<T, U>(result: DecodeResult<T>, f: impl FnOnce(T) -> U) -> DecodeResult<U> {
    match result {
        Ok(v) => Ok(f(v)),
        Err(Partial { value, error }) => Err(Partial {
            value: value.map(f),
            error,
        }),
    }
}

fn number(tag: &Tag) -> DataResult<mc_nbt::Number> {
    tag.as_number().ok_or_else(|| "Not a number".to_string())
}

/// `Codec.INT` (`Number.intValue()` of the boxed value: floats truncate).
pub fn int(tag: &Tag) -> DataResult<i32> {
    number(tag).map(|n| n.boxed_int_value())
}

/// `Codec.LONG`.
pub fn long(tag: &Tag) -> DataResult<i64> {
    number(tag).map(|n| n.boxed_long_value())
}

/// `Codec.FLOAT`.
pub fn float(tag: &Tag) -> DataResult<f32> {
    number(tag).map(|n| n.float_value())
}

/// `Codec.DOUBLE`.
pub fn double(tag: &Tag) -> DataResult<f64> {
    number(tag).map(|n| n.double_value())
}

/// `Codec.BOOL` (`NbtOps.getBooleanValue`: any non-zero number).
pub fn boolean(tag: &Tag) -> DataResult<bool> {
    number(tag).map(|n| n.double_value() != 0.0)
}

/// `Codec.STRING`.
pub fn string(tag: &Tag) -> DataResult<&str> {
    tag.as_string().ok_or_else(|| "Not a string".to_string())
}

/// `Identifier.CODEC`.
pub fn identifier(tag: &Tag) -> DataResult<Identifier> {
    Identifier::read(string(tag)?)
}

pub fn compound(tag: &Tag) -> DataResult<&CompoundTag> {
    tag.as_compound().ok_or_else(|| format!("Not a map: {tag}"))
}

/// `Codec.xxx.listOf()` input: a list tag (arrays are accepted as lists like `NbtOps.getList`).
pub fn list(tag: &Tag) -> DataResult<Vec<Tag>> {
    match tag {
        Tag::List(l) => Ok(l.iter().cloned().collect()),
        Tag::ByteArray(v) => Ok(v.iter().map(|&x| Tag::Byte(x)).collect()),
        Tag::IntArray(v) => Ok(v.iter().map(|&x| Tag::Int(x)).collect()),
        Tag::LongArray(v) => Ok(v.iter().map(|&x| Tag::Long(x)).collect()),
        _ => Err(format!("Not a list: {tag}")),
    }
}

pub fn list_tag(tag: &Tag) -> DataResult<&ListTag> {
    tag.as_list().ok_or_else(|| format!("Not a list: {tag}"))
}

/// `fieldOf(name)`: the field must exist.
pub fn field<'a>(map: &'a CompoundTag, name: &str) -> DataResult<&'a Tag> {
    map.get(name)
        .ok_or_else(|| format!("No key {name} in MapLike[{}]", Tag::Compound(map.clone())))
}

/// `optionalFieldOf(name)`: absent is `None`, present-but-invalid is an error.
pub fn optional<'a, T>(
    map: &'a CompoundTag,
    name: &str,
    parse: impl FnOnce(&'a Tag) -> DataResult<T>,
) -> DataResult<Option<T>> {
    map.get(name).map(parse).transpose()
}

/// `Codec.unboundedMap(key, value)` decode: entries that fail to decode are dropped and reported as one error
/// carrying the entries that did decode (vanilla's partial result).
pub fn unbounded_map<K, V>(
    tag: &Tag,
    mut key: impl FnMut(&str) -> DataResult<K>,
    mut value: impl FnMut(&Tag) -> DataResult<V>,
) -> DecodeResult<Vec<(K, V)>> {
    let m = compound(tag)?;
    let mut ok = Vec::with_capacity(m.len());
    let mut errors = Vec::new();
    for (k, v) in m.iter() {
        match key(k).and_then(|k| Ok((k, value(v)?))) {
            Ok(entry) => ok.push(entry),
            Err(e) => errors.push(format!("{e} missed input: {{{k:?}:{v}}}")),
        }
    }
    if errors.is_empty() {
        Ok(ok)
    } else {
        Err(Partial {
            value: Some(ok),
            error: errors.join("; "),
        })
    }
}
