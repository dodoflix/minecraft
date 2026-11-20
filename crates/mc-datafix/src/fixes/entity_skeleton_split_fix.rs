//! `EntitySkeletonSplitFix`: renames wither and stray skeletons by `SkeletonType`.

use crate::datafix::DataFix;
use crate::fixes::simple_entity_rename_fix;
use crate::schema::Schema;

/// `new EntitySkeletonSplitFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    simple_entity_rename_fix::new(
        "EntitySkeletonSplitFix",
        "EntitySkeletonSplitFix",
        output_schema,
        changes_type,
        |name, tag| {
            let mut name = name;
            if name == "Skeleton" {
                match tag.get("SkeletonType").as_int_or(0) {
                    1 => name = "WitherSkeleton",
                    2 => name = "Stray",
                    _ => {}
                }
            }
            (name.to_string(), tag)
        },
    )
}
