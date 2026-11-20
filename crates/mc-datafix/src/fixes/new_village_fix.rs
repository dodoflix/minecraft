//! `NewVillageFix`: renames the `New_Village` structure start and reference to `Village` (dropping the old `Village`).

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use crate::value::Value;

/// `new NewVillageFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("NewVillageFix", output_schema, changes_type, |df| {
        let input_schema = df.input_schema();
        let starts_type =
            DSL::compound_list_type(&input_schema.get_type(&References::STRUCTURE_FEATURE));
        let chunk_type = input_schema.get_type(&References::CHUNK);
        let structure_type = input_schema.get_type(&References::STRUCTURE_FEATURE);
        let level_finder = chunk_type.find_field("Level");
        let structures_finder = level_finder.ty().find_field("Structures");
        let starts_finder = structures_finder.ty().find_field("Starts");
        let list_finder = starts_type.finder();
        TypeRewriteRule::seq(
            df.fix_type_everywhere_typed("NewVillageFix", &chunk_type, move |input| {
                input.update_typed(&level_finder, |level| {
                    level.update_typed(&structures_finder, |structures| {
                        structures
                            .update_typed(&starts_finder, |starts| {
                                starts.update(&list_finder, |list| {
                                    Value::List(
                                        list.into_list()
                                            .into_iter()
                                            .filter(|pair| pair.first().as_lossy_str() != "Village")
                                            .map(|pair| {
                                                pair.map_first(|name| {
                                                    if name.as_lossy_str() == "New_Village" {
                                                        Value::str("Village")
                                                    } else {
                                                        name
                                                    }
                                                })
                                            })
                                            .collect(),
                                    )
                                })
                            })
                            .update_remainder(|tag| {
                                tag.update("References", |references| {
                                    let references = match references.get("New_Village").result() {
                                        Some(village) => {
                                            references.remove("New_Village").set("Village", village)
                                        }
                                        None => references,
                                    };
                                    references.remove("Village")
                                })
                            })
                    })
                })
            }),
            df.fix_type_everywhere_typed("NewVillageStartFix", &structure_type, |input| {
                input.update_remainder(|tag| {
                    tag.update("id", |id| {
                        if ensure_namespaced(id.as_string_or("").as_lossy_str())
                            == "minecraft:new_village"
                        {
                            id.create_string("minecraft:village")
                        } else {
                            id
                        }
                    })
                })
            }),
        )
    })
}
