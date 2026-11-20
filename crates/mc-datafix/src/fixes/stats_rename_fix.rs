//! `StatsRenameFix`: renames custom statistics (`STATS`) and custom criteria (`OBJECTIVE`).

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::value::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// `new StatsRenameFix(outputSchema, name, renames)`.
pub fn new(output_schema: &Schema, name: &str, renames: &[(&str, &str)]) -> DataFix {
    let name = name.to_string();
    let renames: Arc<HashMap<String, String>> = Arc::new(
        renames
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect(),
    );
    DataFix::new("StatsRenameFix", output_schema, false, move |df| {
        TypeRewriteRule::seq(
            create_stat_rule(df, &name, &renames),
            create_criteria_rule(df, &name, &renames),
        )
    })
}

fn rename(renames: &HashMap<String, String>, value: Value) -> Value {
    Value::Str(
        value
            .as_java_string()
            .map_lossless(|t| renames.get(t).cloned().unwrap_or_else(|| t.to_owned())),
    )
}

fn create_criteria_rule(
    df: &DataFix,
    name: &str,
    renames: &Arc<HashMap<String, String>>,
) -> TypeRewriteRule {
    let output_type = df.output_schema().get_type(&References::OBJECTIVE);
    let input_type = df.input_schema().get_type(&References::OBJECTIVE);
    let criteria_type_finder = input_type.find_field("CriteriaType");
    let Some(choice_type) = criteria_type_finder.ty().find_choice_type("type", -1) else {
        panic!("java.lang.IllegalStateException: Can't find choice type for criteria");
    };
    let Some(custom_field_type) = choice_type.get("minecraft:custom") else {
        panic!("java.lang.IllegalStateException: Failed to find custom criterion type variant");
    };
    let custom_type_finder = DSL::named_choice("minecraft:custom", custom_field_type);
    let id_finder = DSL::field_finder("id", &DSL::namespaced_string());
    let renames = renames.clone();
    df.fix_type_everywhere_typed_new(name, &input_type, &output_type, move |input| {
        input.update_typed(&criteria_type_finder, |stats| {
            stats.update_typed(&custom_type_finder, |custom| {
                custom.update(&id_finder, |value| rename(&renames, value))
            })
        })
    })
}

fn create_stat_rule(
    df: &DataFix,
    name: &str,
    renames: &Arc<HashMap<String, String>>,
) -> TypeRewriteRule {
    let output_type = df.output_schema().get_type(&References::STATS);
    let input_type = df.input_schema().get_type(&References::STATS);
    let stats_finder = input_type.find_field("stats");
    let custom_finder = stats_finder.ty().find_field("minecraft:custom");
    let name_finder = DSL::namespaced_string().finder();
    let renames = renames.clone();
    df.fix_type_everywhere_typed_new(name, &input_type, &output_type, move |input| {
        input.update_typed(&stats_finder, |stats| {
            stats.update_typed(&custom_finder, |custom| {
                custom.update(&name_finder, |value| rename(&renames, value))
            })
        })
    })
}
