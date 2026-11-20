//! `EntityPaintingMotiveFix`: lower-cases and namespaces the `Motive` of paintings, renaming three legacy motives.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_dfu::Dynamic;

/// `new EntityPaintingMotiveFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "EntityPaintingMotiveFix",
        output_schema,
        changes_type,
        "EntityPaintingMotiveFix",
        &References::ENTITY,
        "minecraft:painting",
        |entity| entity.update_remainder(fix_tag),
    )
}

/// `MAP.getOrDefault(motive, motive)`.
fn map_motive(motive: &str) -> &str {
    match motive {
        "donkeykong" => "donkey_kong",
        "burningskull" => "burning_skull",
        "skullandroses" => "skull_and_roses",
        other => other,
    }
}

/// `fixTag(input)`.
pub fn fix_tag(input: Dynamic) -> Dynamic {
    match input.get("Motive").as_string().result() {
        Some(motive) => {
            let fixed = motive.map_lossless(|t| ensure_namespaced(map_motive(&t.to_lowercase())));
            input.set("Motive", input.create_string(fixed))
        }
        None => input,
    }
}
