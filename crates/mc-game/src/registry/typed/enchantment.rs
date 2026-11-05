//! The `enchantment` registry: typed [`Enchantment`] values and the exact `registry_data` NBT (`Enchantment.DIRECT_CODEC`
//! encoded with `NbtOps`).

use mc_dfu::{Dyn, JSON, NBT};
use serde_json::Value;

use crate::registry::codec::RegistryDecodeContext;
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;
use crate::world::item::enchantment::codecs::with_decode_context;
use crate::world::item::enchantment::enchantment::Enchantment;

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let input = Dyn::from_json_str(&json.to_string())?;
    let codec = Enchantment::codec();
    let value = crate::world::level::storage::loot::holder::with_decode_context(ctx, || with_decode_context(ctx, || codec.parse(&JSON, &input))).into_result()?;
    let network = codec.encode_start(&NBT, &value).into_result()?.to_tag()?;
    Ok(DecodedEntry::with_network(value, network))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::ENCHANTMENT, decode };
