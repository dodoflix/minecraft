//! `FeatureFlagRemoveFix`: moves the given feature flags from the enabled to the removed features of a level.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;
use std::collections::HashSet;
use std::sync::Arc;

/// `new FeatureFlagRemoveFix(outputSchema, name, flagsToRemove)`.
pub fn new(output_schema: &Schema, name: &str, flags_to_remove: &[&str]) -> DataFix {
    let name = name.to_string();
    let flags: Arc<HashSet<String>> =
        Arc::new(flags_to_remove.iter().map(|f| f.to_string()).collect());
    DataFix::new("FeatureFlagRemoveFix", output_schema, false, move |df| {
        let flags = flags.clone();
        df.fix_type_everywhere_typed(
            &name,
            &df.input_schema().get_type(&References::LIGHTWEIGHT_LEVEL),
            move |input| input.update_remainder(|tag| fix_tag(tag, &flags)),
        )
    })
}

fn fix_tag(tag: Dynamic, flags_to_remove: &HashSet<String>) -> Dynamic {
    let mut inactive_features: Vec<Dynamic> = tag.get("removed_features").as_stream();
    let source = tag.clone();
    let result = tag.update("enabled_features", |features| {
        match features.as_stream_opt().result() {
            Some(stream) => {
                let mut kept = Vec::new();
                for feature in stream {
                    match feature.as_string().result() {
                        None => kept.push(feature),
                        Some(id) => {
                            if flags_to_remove.contains(id.as_lossy_str()) {
                                inactive_features.push(source.create_string(id));
                            } else {
                                kept.push(feature);
                            }
                        }
                    }
                }
                source.create_list(kept)
            }
            None => features,
        }
    });
    if inactive_features.is_empty() {
        result
    } else {
        result.set("removed_features", source.create_list(inactive_features))
    }
}
