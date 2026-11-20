//! `PrimedTntBlockStateFixer`: gives primed TNT a block state and renames its fuse field.

use crate::datafix::DataFix;
use crate::fixes::named_entity_write_read_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new PrimedTntBlockStateFixer(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_write_read_fix::new(
        "PrimedTntBlockStateFixer",
        output_schema,
        true,
        "PrimedTnt BlockState fixer",
        &References::ENTITY,
        "minecraft:tnt",
        |input| rename_fuse(insert_block_state(input)),
    )
}

fn rename_fuse(input: Dynamic) -> Dynamic {
    match input.get("Fuse").result() {
        Some(fuse_value) => input.set("fuse", fuse_value),
        None => input,
    }
}

fn insert_block_state(input: Dynamic) -> Dynamic {
    let state = input.create_map([(
        input.create_string("Name"),
        input.create_string("minecraft:tnt"),
    )]);
    input.set("block_state", state)
}
