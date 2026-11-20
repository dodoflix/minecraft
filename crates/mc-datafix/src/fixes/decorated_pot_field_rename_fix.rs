//! `DecoratedPotFieldRenameFix`: converts the decorated pot block entity to the type of the output schema.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new DecoratedPotFieldRenameFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("DecoratedPotFieldRenameFix", output_schema, true, |df| {
        let old_decorated_pot = df
            .input_schema()
            .get_choice_type(&References::BLOCK_ENTITY, "minecraft:decorated_pot");
        let new_decorated_pot = df
            .output_schema()
            .get_choice_type(&References::BLOCK_ENTITY, "minecraft:decorated_pot");
        df.convert_unchecked(
            "DecoratedPotFieldRenameFix",
            &old_decorated_pot,
            &new_decorated_pot,
        )
    })
}
