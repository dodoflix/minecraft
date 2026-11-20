//! `InvalidBlockEntityLockFix`: drops block entity `lock` predicates that only match an empty custom name.

use crate::datafix::DataFix;
use crate::fixes::invalid_lock_component_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new InvalidBlockEntityLockFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("InvalidBlockEntityLockFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "BlockEntityLockToComponentFix",
            &df.input_schema().get_type(&References::BLOCK_ENTITY),
            |block_entity| {
                block_entity.update_remainder(|remainder| match remainder.get("lock").result() {
                    None => remainder,
                    Some(lock) => match invalid_lock_component_fix::fix_lock(lock) {
                        Some(new_lock) => remainder.set("lock", new_lock),
                        None => remainder.remove("lock"),
                    },
                })
            },
        )
    })
}
