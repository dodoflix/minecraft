//! Type erasure for component values and the per-component definition (`DataComponentType`).

use crate::codec::EncodeResult;
use crate::codec::{DecodeResult, NbtLimit};
use mc_data::DataComponentType;
use mc_dfu::{Codec, DataResult};
use std::any::Any;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

/// What a component value must be: cloneable, comparable (vanilla `equals`), printable.
pub trait ComponentValue: Any + Send + Sync + fmt::Debug {
    fn eq_value(&self, other: &dyn ComponentValue) -> bool;
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any + Send + Sync + fmt::Debug + PartialEq> ComponentValue for T {
    fn eq_value(&self, other: &dyn ComponentValue) -> bool {
        other.as_any().downcast_ref::<T>().is_some_and(|o| self == o)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A type-erased, shareable component value (vanilla stores `Object`).
#[derive(Clone)]
pub struct Value(Arc<dyn ComponentValue>);

impl Value {
    pub fn new<T: ComponentValue>(v: T) -> Value {
        Value(Arc::new(v))
    }
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.0.as_any().downcast_ref::<T>()
    }
    /// Typed clone of the value; panics if the type is wrong (a bug in a `ComponentDef`).
    pub fn cloned<T: Clone + 'static>(&self) -> T {
        self.downcast_ref::<T>().unwrap_or_else(|| panic!("component value is not a {}: {:?}", std::any::type_name::<T>(), self.0)).clone()
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.eq_value(&*other.0)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// `DataComponentType<T>` as a compile-time typed key (the runtime identity is the `mc_data` registry id).
pub struct ComponentKey<T> {
    id: DataComponentType,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for ComponentKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for ComponentKey<T> {}

impl<T> ComponentKey<T> {
    pub const fn new(id: DataComponentType) -> Self {
        ComponentKey { id, _marker: PhantomData }
    }
    pub const fn id(&self) -> DataComponentType {
        self.id
    }
}

type NetEncode = Box<dyn Fn(&Value, &mut Vec<u8>) -> EncodeResult<()> + Send + Sync>;
type NetDecode = Box<dyn Fn(&mut &[u8]) -> DecodeResult<Value> + Send + Sync>;

/// Everything vanilla's `DataComponentType.Builder` holds: persistent codec (None = transient), the network stream
/// codec (explicit or derived from the codec, like `fromCodecWithRegistries`).
pub struct ComponentDef {
    pub id: DataComponentType,
    pub codec: Option<Codec<Value>>,
    /// `codec` producing `Some(value)`: the value codec of a non-removed `DataComponentPatch` entry.
    pub(crate) opt_codec: Option<Codec<Option<Value>>>,
    pub(crate) net_encode: NetEncode,
    pub(crate) net_decode: NetDecode,
}

impl ComponentDef {
    pub fn name(&self) -> &'static str {
        self.id.name()
    }
    pub fn is_transient(&self) -> bool {
        self.codec.is_none()
    }
    pub fn ignore_swap_animation(&self) -> bool {
        self.id.data().ignore_swap_animation
    }
    /// `codecOrThrow()`.
    pub fn codec_or_throw(&self) -> &Codec<Value> {
        self.codec.as_ref().unwrap_or_else(|| panic!("{} is not a persistent component", self.name()))
    }
    pub fn encode_network(&self, value: &Value, out: &mut Vec<u8>) -> EncodeResult<()> {
        (self.net_encode)(value, out)
    }
    pub fn decode_network(&self, input: &mut &[u8]) -> DecodeResult<Value> {
        (self.net_decode)(input)
    }
}

/// Builder: `ComponentDef::builder(KEY).persistent(codec).network_synchronized()`.
pub struct ComponentDefBuilder<T: 'static> {
    id: DataComponentType,
    codec: Option<Codec<T>>,
    net: Option<(NetEncode, NetDecode)>,
}

impl ComponentDef {
    pub fn builder<T: ComponentValue + Clone>(key: ComponentKey<T>) -> ComponentDefBuilder<T> {
        ComponentDefBuilder { id: key.id, codec: None, net: None }
    }
}

impl<T: ComponentValue + Clone> ComponentDefBuilder<T> {
    /// `persistent(codec)`.
    pub fn persistent(mut self, codec: Codec<T>) -> Self {
        self.codec = Some(codec);
        self
    }

    /// `networkSynchronized(streamCodec)` with `T`'s `Encode`/`Decode` as the stream codec.
    pub fn network_synchronized(mut self) -> Self
    where
        T: crate::codec::Encode + crate::codec::Decode,
    {
        self.net = Some((
            Box::new(|v, out| v.cloned::<T>().encode(out)),
            Box::new(|input| <T as crate::codec::Decode>::decode(input).map(Value::new)),
        ));
        self
    }

    /// `networkSynchronized(streamCodec)` with explicit functions.
    pub fn network_with(
        mut self,
        encode: impl Fn(&T, &mut Vec<u8>) -> EncodeResult<()> + Send + Sync + 'static,
        decode: impl Fn(&mut &[u8]) -> DecodeResult<T> + Send + Sync + 'static,
    ) -> Self {
        self.net = Some((
            Box::new(move |v, out| encode(v.downcast_ref::<T>().expect("component value type"), out)),
            Box::new(move |input| decode(input).map(Value::new)),
        ));
        self
    }

    pub fn build(self) -> ComponentDef {
        let id = self.id;
        let codec_t = self.codec;
        let codec = codec_t.as_ref().map(|c| {
            c.xmap(Value::new, |v: &Value| v.cloned::<T>())
        });
        let (net_encode, net_decode) = match self.net {
            Some(n) => n,
            None => {
                // `ByteBufCodecs.fromCodecWithRegistries(codec)`: the value as an NBT tag
                let c = codec_t.expect("Missing Codec for component");
                let c2 = c.clone();
                (
                    Box::new(move |v: &Value, out: &mut Vec<u8>| {
                        let tag = mc_dfu::encode_nbt(&c, &v.cloned::<T>());
                        match tag.into_result() {
                            Ok(t) => crate::codec::encode_tag(&t, out),
                            Err(e) => Err(crate::codec::EncodeError(format!("Failed to encode: {e}"))),
                        }
                    }) as NetEncode,
                    Box::new(move |input: &mut &[u8]| {
                        let tag = crate::codec::decode_tag_with(input, NbtLimit::DEFAULT)?
                            .ok_or_else(|| crate::codec::DecodeError::msg("Expected non-null compound tag"))?;
                        match mc_dfu::parse_nbt(&c2, &tag) {
                            DataResult::Success(v) => Ok(Value::new(v)),
                            e => Err(crate::codec::DecodeError::Message(format!(
                                "Failed to decode: {} {tag}",
                                e.error_message().unwrap_or_default()
                            ))),
                        }
                    }) as NetDecode,
                )
            }
        };
        let opt_codec = codec.as_ref().map(|c| c.xmap(Some, |v: &Option<Value>| v.clone().expect("removed entries use the removed codec")));
        ComponentDef { id, codec, opt_codec, net_encode, net_decode }
    }
}
