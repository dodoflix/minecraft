//! `EntityFallDistanceFloatToDoubleFix`: renames `FallDistance` to `fall_distance` and widens it to a double.

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::schema::Schema;
use crate::typed::Typed;

/// `new EntityFallDistanceFloatToDoubleFix(outputSchema, type)`.
pub fn new(output_schema: &Schema, ty: &TypeReference) -> DataFix {
    let ty = *ty;
    DataFix::new(
        "EntityFallDistanceFloatToDoubleFix",
        output_schema,
        false,
        move |df| {
            df.fix_type_everywhere_typed(
                &format!("EntityFallDistanceFloatToDoubleFixFor{}", ty.type_name()),
                &df.output_schema().get_type(&ty),
                fix_entity,
            )
        },
    )
}

fn fix_entity(entity: Typed) -> Typed {
    entity.update_remainder(|remainder| {
        remainder.rename_and_fix_field("FallDistance", "fall_distance", |fall_distance| {
            fall_distance.create_double(fall_distance.as_float_or(0.0) as f64)
        })
    })
}
