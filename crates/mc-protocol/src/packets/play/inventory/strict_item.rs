//! `Item.STREAM_CODEC` is `ByteBufCodecs.holderRegistry`: unlike `registry(..)` it rejects unknown ids even though the
//! item registry is a defaulted one. For `#[mc(with = ..)]`.
use mc_data::Item;

use crate::codec::{read_count, read_registry_id, DecodeResult};
use crate::Encode;

pub fn encode(item: &Item, out: &mut Vec<u8>) {
    item.encode(out)
}
pub fn decode(input: &mut &[u8]) -> DecodeResult<Item> {
    read_registry_id(input)
}

/// `Item.STREAM_CODEC.apply(ByteBufCodecs.list())`
pub fn decode_list(input: &mut &[u8]) -> DecodeResult<Vec<Item>> {
    let count = read_count(input, i32::MAX as usize)?;
    (0..count).map(|_| decode(input)).collect()
}
