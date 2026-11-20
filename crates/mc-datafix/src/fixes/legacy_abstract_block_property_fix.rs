//! `LegacyAbstractBlockPropertyFix`: rewrites the `Properties` of the block states of the blocks `should_fix`
//! accepts. Abstract in Java; `should_fix` and `fix_properties` are the subclass's `shouldFix(blockId)` and
//! `fixProperties(blockId, properties)`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_dfu::Dynamic;
use std::sync::Arc;

/// `new LegacyAbstractBlockPropertyFix(outputSchema, name)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    should_fix: impl Fn(&str) -> bool + Send + Sync + 'static,
    fix_properties: impl Fn(&str, Dynamic) -> Dynamic + Send + Sync + 'static,
) -> DataFix {
    let name = name.to_string();
    let (should_fix, fix_properties) = (Arc::new(should_fix), Arc::new(fix_properties));
    DataFix::new(class, output_schema, false, move |df| {
        let (should_fix, fix_properties) = (should_fix.clone(), fix_properties.clone());
        df.fix_type_everywhere_typed(
            &name,
            &df.input_schema().get_type(&References::BLOCK_STATE),
            move |input| {
                input.update_remainder(|tag| {
                    let block_id = tag
                        .get("Name")
                        .as_string()
                        .result()
                        .map(|n| ensure_namespaced(n.as_lossy_str()));
                    match block_id {
                        Some(id) if should_fix(&id) => {
                            tag.update("Properties", |properties| fix_properties(&id, properties))
                        }
                        _ => tag,
                    }
                })
            },
        )
    })
}
