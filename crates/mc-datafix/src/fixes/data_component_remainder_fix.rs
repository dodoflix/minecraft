//! `DataComponentRemainderFix`: rewrites one data component (optionally under a new id) in the remainder of
//! `DATA_COMPONENTS`. Abstract in Java; `fix_component` is the subclass's `fixComponent` (`None` = Java `null`: the
//! component is dropped).

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;
use std::sync::Arc;

/// `new DataComponentRemainderFix(outputSchema, name, componentId)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    name: &str,
    component_id: &str,
    fix_component: impl Fn(Dynamic) -> Option<Dynamic> + Send + Sync + 'static,
) -> DataFix {
    new_renamed(
        class,
        output_schema,
        name,
        component_id,
        component_id,
        fix_component,
    )
}

/// `new DataComponentRemainderFix(outputSchema, name, componentId, newComponentId)` of the subclass `class`.
pub fn new_renamed(
    class: &str,
    output_schema: &Schema,
    name: &str,
    component_id: &str,
    new_component_id: &str,
    fix_component: impl Fn(Dynamic) -> Option<Dynamic> + Send + Sync + 'static,
) -> DataFix {
    let (name, component_id, new_component_id) = (
        name.to_string(),
        component_id.to_string(),
        new_component_id.to_string(),
    );
    let fix_component = Arc::new(fix_component);
    DataFix::new(class, output_schema, false, move |df| {
        let data_components_type = df.input_schema().get_type(&References::DATA_COMPONENTS);
        let (fix_component, component_id, new_component_id) = (
            fix_component.clone(),
            component_id.clone(),
            new_component_id.clone(),
        );
        df.fix_type_everywhere_typed(&name, &data_components_type, move |components| {
            components.update_remainder(|remainder| match remainder.get(&component_id).result() {
                None => remainder,
                Some(component) => remainder
                    .remove(&component_id)
                    .set_field_if_present(&new_component_id, fix_component(component)),
            })
        })
    })
}
