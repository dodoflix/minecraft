//! The `recipe` registry's typed decoder (`Recipe.DIRECT_CODEC` over `JsonOps`).

use std::collections::HashMap;
use std::sync::LazyLock;

use mc_core::ident::Identifier;
use mc_dfu::{Codec, Dyn, JSON};
use serde_json::Value;

use super::ingredient::with_item_tag_lookup;
use super::recipe::Recipe;
use crate::registry::codec::{Lookup, RegistryDecodeContext};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

static CODEC: LazyLock<Codec<Recipe>> = LazyLock::new(Recipe::codec);

/// Every `#tag` string of `json` (the ingredient tag references), looked up in the item registry.
fn collect_item_tags(
    json: &Value,
    ctx: &RegistryDecodeContext,
    out: &mut HashMap<Identifier, bool>,
) {
    match json {
        Value::String(s) => {
            if let Some(tag) = s.strip_prefix('#').and_then(Identifier::try_parse) {
                out.entry(tag.clone())
                    .or_insert_with(|| ctx.lookup.tag_exists(keys::ITEM, &tag) == Lookup::Yes);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| collect_item_tags(v, ctx, out)),
        Value::Object(map) => map.values().for_each(|v| collect_item_tags(v, ctx, out)),
        _ => {}
    }
}

/// Decodes one `recipe/<id>.json`.
pub fn decode_recipe(json: &Value, ctx: &RegistryDecodeContext) -> Result<Recipe, String> {
    let mut tags = HashMap::new();
    collect_item_tags(json, ctx, &mut tags);
    let dynamic = Dyn::from_json_str(&json.to_string())?;
    with_item_tag_lookup(tags, || CODEC.parse(&JSON, &dynamic).into_result())
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static RECIPE_DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::RECIPE,
    decode: |json, ctx| decode_recipe(json, ctx).map(DecodedEntry::new),
};
