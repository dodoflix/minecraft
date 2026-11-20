//! `PotDecorationsBlockEntityUnflatteningFix`: the `sherds` of a decorated pot become a map keyed by side.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::pot_decorations_component_unflattening_fix::unpack_list;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;

/// `new PotDecorationsBlockEntityUnflatteningFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    let schema = output_schema.clone();
    named_entity_fix::new(
        "PotDecorationsBlockEntityUnflatteningFix",
        output_schema,
        true,
        "PotDecorationsBlockEntityUnflatteningFix",
        &References::BLOCK_ENTITY,
        "minecraft:decorated_pot",
        move |entity| {
            let new_type = schema
                .find_choice_type(&References::BLOCK_ENTITY)
                .get("minecraft:decorated_pot")
                .cloned()
                .expect("minecraft:decorated_pot is a block entity of the output schema");
            write_and_read_typed_or_throw(&entity, &new_type, |contents| {
                let original = contents.get("sherds").or_else_empty_list();
                contents.set("sherds", unpack_list(&original))
            })
        },
    )
}
