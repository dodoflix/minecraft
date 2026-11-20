//! `FixWolfHealth`: doubles the health of wolves whose maximum health attribute is the old default.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_dfu::Dynamic;

const WOLF_HEALTH: &str = "minecraft:generic.max_health";

/// `new FixWolfHealth(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "FixWolfHealth",
        output_schema,
        false,
        "FixWolfHealth",
        &References::ENTITY,
        "minecraft:wolf",
        |entity| {
            entity.update_remainder(|dynamic| {
                let mut health_adjusted = false;
                let dynamic = dynamic.update("Attributes", |attributes| {
                    let fixed: Vec<Dynamic> = attributes
                        .as_stream()
                        .into_iter()
                        .map(|attribute| {
                            if WOLF_HEALTH
                                == ensure_namespaced(&attribute.get("Name").as_string_or(""))
                            {
                                attribute.update("Base", |base| {
                                    if base.as_double_or(0.0) == 20.0 {
                                        health_adjusted = true;
                                        base.create_double(40.0)
                                    } else {
                                        base
                                    }
                                })
                            } else {
                                attribute
                            }
                        })
                        .collect();
                    attributes.create_list(fixed)
                });
                if health_adjusted {
                    dynamic.update("Health", |health| {
                        health.create_float(health.as_float_or(0.0) * 2.0)
                    })
                } else {
                    dynamic
                }
            })
        },
    )
}
