//! Installs the typed normalizers `mc_text` needs for `show_item` components and `show_dialog` payloads
//! (`mc_text::set_payload_hooks`): the payload is decoded with the typed codec of its input ops and re-encoded as NBT, so
//! `{"minecraft:damage":3}` from JSON becomes an int tag, invalid components are rejected, unknown ones rejected, ...

use super::common::{dyn_to_serde, serde_to_dyn};
use super::map::DataComponentPatch;
use crate::dialog::Dialog;
use mc_dfu::{Dyn, JSON, NBT};
use mc_nbt::Tag;
use mc_text::{PayloadHooks, PayloadKind};

/// `Dialog.CODEC` parse with `ops`, stored as `Holder<Dialog>` in NBT form (a registry id string or the dialog compound).
fn normalize_dialog(ops: &mc_dfu::Ops, d: &Dyn) -> Result<Tag, String> {
    let holder = Dialog::codec().parse(ops, d).into_result()?;
    mc_dfu::encode_nbt(&Dialog::codec(), &holder).into_result()
}

fn from_json(kind: PayloadKind, v: &serde_json::Value) -> Result<Tag, String> {
    let d = serde_to_dyn(v);
    match kind {
        PayloadKind::ItemComponents => {
            let patch = DataComponentPatch::codec().parse(&JSON, &d).into_result()?;
            mc_dfu::encode_nbt(&DataComponentPatch::codec(), &patch).into_result()
        }
        PayloadKind::Dialog => normalize_dialog(&JSON, &d),
    }
}

fn from_nbt(kind: PayloadKind, t: &Tag) -> Result<Tag, String> {
    match kind {
        PayloadKind::ItemComponents => {
            let patch = mc_dfu::parse_nbt(&DataComponentPatch::codec(), t).into_result()?;
            mc_dfu::encode_nbt(&DataComponentPatch::codec(), &patch).into_result()
        }
        PayloadKind::Dialog => normalize_dialog(&NBT, &Dyn::from(t)),
    }
}

fn to_json(kind: PayloadKind, t: &Tag) -> Result<serde_json::Value, String> {
    match kind {
        PayloadKind::ItemComponents => {
            // The stored compound keeps the typed patch's insertion order (vanilla's `Reference2ObjectArrayMap`), which is
            // observable in the JSON output; only the top level is ordered that way, nested compounds are `HashMap`s.
            let ordered = match t {
                Tag::Compound(c) => Dyn::Map(
                    c.iter()
                        .map(|(k, v)| (Dyn::Str(k.clone()), Dyn::from(v)))
                        .collect(),
                ),
                other => Dyn::from(other),
            };
            let patch = DataComponentPatch::codec()
                .parse(&NBT, &ordered)
                .into_result()?;
            let d: Dyn = DataComponentPatch::codec()
                .encode_start(&JSON, &patch)
                .into_result()?;
            Ok(dyn_to_serde(&d))
        }
        // a registry reference was validated when it was parsed
        PayloadKind::Dialog if matches!(t, Tag::String(_)) => {
            Ok(dyn_to_serde(&NBT.convert_to(&JSON, &Dyn::from(t))))
        }
        PayloadKind::Dialog => {
            let dialog = Dialog::direct_codec()
                .parse(&NBT, &Dyn::from(t))
                .into_result()?;
            Ok(dyn_to_serde(
                &Dialog::direct_codec()
                    .encode_start(&JSON, &dialog)
                    .into_result()?,
            ))
        }
    }
}

/// Install the hooks into `mc_text` (call once at startup, like `mc_text::set_validators`).
pub fn install_text_hooks() {
    mc_text::set_payload_hooks(PayloadHooks {
        from_json,
        from_nbt,
        to_json,
    });
}
