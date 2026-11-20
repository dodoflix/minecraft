//! `CriteriaRenameFix`: renames the criteria of one advancement.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;
use std::sync::Arc;

/// `new CriteriaRenameFix(outputSchema, name, advancementId, conversions)`.
pub fn new(
    output_schema: &Schema,
    name: &str,
    advancement_id: &str,
    conversions: impl Fn(&str) -> String + Send + Sync + 'static,
) -> DataFix {
    let (name, advancement_id) = (name.to_string(), advancement_id.to_string());
    let conversions = Arc::new(conversions);
    DataFix::new("CriteriaRenameFix", output_schema, false, move |df| {
        let (advancement_id, conversions) = (advancement_id.clone(), conversions.clone());
        df.fix_type_everywhere_typed(
            &name,
            &df.input_schema().get_type(&References::ADVANCEMENTS),
            move |input| {
                input.update_remainder(|tag| fix_advancements(tag, &advancement_id, &*conversions))
            },
        )
    })
}

fn fix_advancements(
    tag: Dynamic,
    advancement_id: &str,
    conversions: &dyn Fn(&str) -> String,
) -> Dynamic {
    tag.update(advancement_id, |advancement| {
        advancement.update("criteria", |criteria| {
            criteria.update_map_values(|(key, value)| {
                let converted = key
                    .as_string()
                    .map(|s| key.create_string(conversions(s.as_lossy_str())))
                    .result();
                (converted.unwrap_or(key), value)
            })
        })
    })
}
