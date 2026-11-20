//! `SignsOpFlagFix`: signs and hanging signs get `allow_op_features`.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;

/// `new SignsOpFlagFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("SignsOpFlagFix", output_schema, false, |df| {
        TypeRewriteRule::seq(
            make_rule_for_type(df, "minecraft:sign"),
            make_rule_for_type(df, "minecraft:hanging_sign"),
        )
    })
}

fn make_rule_for_type(df: &DataFix, entity_name: &str) -> TypeRewriteRule {
    let entity_type = df
        .input_schema()
        .get_choice_type(&References::BLOCK_ENTITY, entity_name);
    let entity_f = DSL::named_choice(entity_name, &entity_type);
    df.fix_type_everywhere_typed(
        &format!("Add allow-op flag to {entity_name}"),
        &df.input_schema().get_type(&References::BLOCK_ENTITY),
        move |input| {
            input.update_typed_as(&entity_f, &entity_type, |entity| {
                entity.update_remainder(|remainder| {
                    remainder.set("allow_op_features", remainder.create_boolean(true))
                })
            })
        },
    )
}
