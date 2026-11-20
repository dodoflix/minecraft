//! `ThrownPotionSplitFix`: splits `minecraft:potion` entities into `minecraft:splash_potion` and
//! `minecraft:lingering_potion` by the item they hold.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::patch_sub_type;
use crate::finder::OpticFinder;
use crate::fixes::entity_rename_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use crate::typed::Typed;
use std::sync::OnceLock;

/// `ThrownPotionSplitFix.ItemIdFinder`.
struct ItemIdFinder {
    item_finder: OpticFinder,
    item_id_finder: OpticFinder,
}

impl ItemIdFinder {
    /// `getItemId(entity)`.
    fn get_item_id(&self, entity: &Typed) -> String {
        entity
            .get_optional_typed(&self.item_finder)
            .and_then(|item| item.get_optional(&self.item_id_finder))
            .map(|id| ensure_namespaced(id.second().as_str()))
            .unwrap_or_default()
    }
}

/// `new ThrownPotionSplitFix(outputSchema)` (`changesType` is `true`).
pub fn new(output_schema: &Schema) -> DataFix {
    let input_schema = output_schema
        .parent()
        .expect("a type-changing fix needs a parent schema")
        .clone();
    let output_schema2 = output_schema.clone();
    let item_id_finder: OnceLock<ItemIdFinder> = OnceLock::new();
    entity_rename_fix::new(
        "ThrownPotionSplitFix",
        "ThrownPotionSplitFix",
        output_schema,
        true,
        move |name, entity| {
            if name != "minecraft:potion" {
                return (name.to_string(), entity);
            }
            let finder = item_id_finder.get_or_init(|| {
                let potion_type =
                    input_schema.get_choice_type(&References::ENTITY, "minecraft:potion");
                let patched_potion_type = patch_sub_type(
                    &potion_type,
                    &input_schema.get_type(&References::ENTITY),
                    &output_schema2.get_type(&References::ENTITY),
                );
                let item_finder = patched_potion_type.find_field("Item");
                let item_id_finder = DSL::field_finder(
                    "id",
                    &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
                );
                ItemIdFinder {
                    item_finder,
                    item_id_finder,
                }
            });
            let item_id = finder.get_item_id(&entity);
            if item_id == "minecraft:lingering_potion" {
                ("minecraft:lingering_potion".to_string(), entity)
            } else {
                ("minecraft:splash_potion".to_string(), entity)
            }
        },
    )
}
