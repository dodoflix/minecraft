//! A Rust port of the parts of Mojang's DataFixerUpper `Codec` framework that vanilla's data codecs use
//! (`DataResult`, `DynamicOps` for NBT/JSON/hash, `Codec`, `MapCodec`, record/dispatch/list/optional-field combinators),
//! so that data component, data-pack and saved-data codecs can be written once and run over every ops.

mod codec;
pub mod dynamic_like;
mod nbt_order;
mod dynv;
pub mod extra;
mod hash;
mod ops;
#[macro_use]
mod record;
mod result;
pub mod builtin;

pub use builtin::*;
pub use codec::{Codec, CodecImpl, Dynamic, Either, MapCodec, MapCodecImpl};
pub use dynv::{Dyn, JNum};
pub use hash::{crc32c, hash_dyn};
pub use ops::{ListBuilder, MapLike, Ops, OpsKind, RecordBuilder, HASH, JSON, NBT};
pub use dynamic_like::OptionalDynamic;
pub use result::{DataResult, Msg};

use mc_nbt::Tag;

/// Parse a value from an NBT tag with `codec`.
pub fn parse_nbt<A: 'static>(codec: &Codec<A>, tag: &Tag) -> DataResult<A> {
    codec.parse(&NBT, &Dyn::from(tag))
}

/// Encode a value to an NBT tag with `codec`.
pub fn encode_nbt<A: 'static>(codec: &Codec<A>, value: &A) -> DataResult<Tag> {
    codec.encode_start(&NBT, value).flat_map(|d| d.to_tag().into())
}

/// Parse a value from JSON text (strict JSON) with `codec`.
pub fn parse_json<A: 'static>(codec: &Codec<A>, text: &str) -> DataResult<A> {
    match Dyn::from_json_str(text) {
        Ok(d) => codec.parse(&JSON, &d),
        Err(e) => DataResult::error(move || format!("Failed to parse JSON: {e}")),
    }
}

/// Encode a value to JSON text (Gson compact form) with `codec`.
pub fn encode_json<A: 'static>(codec: &Codec<A>, value: &A) -> DataResult<String> {
    codec.encode_start(&JSON, value).map(|d| d.to_json_string())
}

/// `HashOps.CRC32C_INSTANCE` hash of `value` (`codec.encodeStart(HashOps, value)`).
pub fn hash_with<A: 'static>(codec: &Codec<A>, value: &A) -> DataResult<u32> {
    codec.encode_start(&HASH, value).map(|d| hash_dyn(&d))
}
