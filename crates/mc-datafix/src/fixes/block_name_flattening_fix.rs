//! `BlockNameFlatteningFix`: replaces legacy numeric and string block names by the flattened block names.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::block_state_data;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use crate::value::Value;

/// `new BlockNameFlatteningFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "BlockNameFlatteningFix",
        output_schema,
        changes_type,
        |df| {
            let block_type = df.input_schema().get_type(&References::BLOCK_NAME);
            let new_block_type = df.output_schema().get_type(&References::BLOCK_NAME);
            let expected_block_type = DSL::named_type(
                References::BLOCK_NAME.type_name(),
                &DSL::or_type(&DSL::int_type(), &DSL::namespaced_string()),
            );
            let expected_new_block_type = DSL::named_type(
                References::BLOCK_NAME.type_name(),
                &DSL::namespaced_string(),
            );
            if block_type == expected_block_type && new_block_type == expected_new_block_type {
                df.fix_type_everywhere_new(
                    "BlockNameFlatteningFix",
                    &expected_block_type,
                    &expected_new_block_type,
                    |_, block| {
                        block.map_second(|choice| match choice {
                            Value::Left(id) => {
                                Value::str(block_state_data::upgrade_block_id(id.as_int()))
                            }
                            Value::Right(name) => Value::str(block_state_data::upgrade_block(
                                &ensure_namespaced(name.as_str()),
                            )),
                            other => panic!("expected an Either, got {other:?}"),
                        })
                    },
                )
            } else {
                panic!("java.lang.IllegalStateException: Expected and actual types don't match.")
            }
        },
    )
}
