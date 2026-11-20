//! `ContainerBlockEntityLockPredicateFix`: the `Lock` string of block entities becomes a `lock` item predicate.

use crate::datafix::DataFix;
use crate::fixes::lock_component_predicate_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;

/// `new ContainerBlockEntityLockPredicateFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "ContainerBlockEntityLockPredicateFix",
        output_schema,
        false,
        |df| {
            df.fix_type_everywhere_typed(
                "ContainerBlockEntityLockPredicateFix",
                df.input_schema()
                    .find_choice_type(&References::BLOCK_ENTITY)
                    .ty(),
                fix_block_entity,
            )
        },
    )
}

fn fix_block_entity(entity: Typed) -> Typed {
    entity.update_remainder(|tag| {
        // `renameAndFixField` with a fixer returning `null`: `Optional.map` drops the field
        tag.remove("Lock").set_field_if_present(
            "lock",
            tag.get("Lock")
                .result()
                .and_then(lock_component_predicate_fix::fix_lock),
        )
    })
}
