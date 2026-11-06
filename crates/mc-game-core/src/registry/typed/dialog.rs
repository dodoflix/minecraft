//! `server.dialog.Dialog`: the `dialog` registry (`RegistryDataLoader` entry with `Dialog.DIRECT_CODEC`, synchronized as is).

use mc_core::ident::Identifier;
use mc_dfu::JSON;
use mc_protocol::dialog::Dialog;
use mc_protocol::registry_context::{ScopedLookup, with_scoped_lookup};
use serde_json::Value;

use crate::registry::codec::{Lookup, RegistryDecodeContext};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// The loading stage's `RegistryOps` lookup: `dialog_list` entries reference other dialogs and `#dialog` tags.
struct StageLookup<'a, 'b>(&'a RegistryDecodeContext<'b>);

impl ScopedLookup for StageLookup<'_, '_> {
    fn contains(&self, registry: &str, name: &Identifier) -> Option<bool> {
        match self.0.lookup.contains(registry, name) {
            Lookup::NoRegistry => None,
            _ => Some(self.0.check_holder(registry, name).is_ok()),
        }
    }

    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Option<bool> {
        match self.0.lookup.tag_exists(registry, tag) {
            Lookup::NoRegistry => None,
            _ => Some(self.0.check_tag(registry, tag).is_ok()),
        }
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    let input = mc_protocol::components::common::serde_to_dyn(json);
    let dialog = with_scoped_lookup(&StageLookup(ctx), || {
        Dialog::direct_codec().parse(&JSON, &input)
    })
    .into_result()?;
    let network = dialog.to_tag()?;
    Ok(DecodedEntry::with_network(dialog, network))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::DIALOG,
    decode,
};
