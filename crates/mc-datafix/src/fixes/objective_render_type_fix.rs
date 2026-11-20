//! `ObjectiveRenderTypeFix`: adds the default `RenderType` to objectives that lack one.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ObjectiveRenderTypeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ObjectiveRenderTypeFix", output_schema, false, |df| {
        let objective_type = df.input_schema().get_type(&References::OBJECTIVE);
        df.fix_type_everywhere_typed("ObjectiveRenderTypeFix", &objective_type, |typed| {
            typed.update_remainder(|tag| {
                if tag.get("RenderType").as_string().result().is_none() {
                    let criteria_name = tag.get("CriteriaName").as_string_or("");
                    let default_render_type = get_render_type(&criteria_name);
                    tag.set("RenderType", tag.create_string(default_render_type))
                } else {
                    tag
                }
            })
        })
    })
}

/// `getRenderType(criteriaName)`.
fn get_render_type(criteria_name: &str) -> &'static str {
    if criteria_name == "health" {
        "hearts"
    } else {
        "integer"
    }
}
