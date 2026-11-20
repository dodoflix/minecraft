//! `PlayerHeadBlockProfileFix`: moves the skull owner of a player head into its `profile`.

use crate::datafix::DataFix;
use crate::fixes::item_stack_componentization_fix::fix_profile;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new PlayerHeadBlockProfileFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "PlayerHeadBlockProfileFix",
        output_schema,
        false,
        "PlayerHeadBlockProfileFix",
        &References::BLOCK_ENTITY,
        "minecraft:skull",
        |entity| entity.update_remainder(fix),
    )
}

fn fix(entity: Dynamic) -> Dynamic {
    let skull_owner = entity.get("SkullOwner").result();
    let extra_type = entity.get("ExtraType").result();
    match skull_owner.or(extra_type) {
        None => entity,
        Some(profile) => {
            let entity = entity.remove("SkullOwner").remove("ExtraType");
            entity.set("profile", fix_profile(&profile))
        }
    }
}
