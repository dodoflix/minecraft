//! `EquippableAssetRenameFix`: renames `model` to `asset_id` in the `minecraft:equippable` component.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new EquippableAssetRenameFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("EquippableAssetRenameFix", output_schema, true, |df| {
        let components_type = df.input_schema().get_type(&References::DATA_COMPONENTS);
        let equippable_field = components_type.find_field("minecraft:equippable");
        df.fix_type_everywhere_typed(
            "equippable asset rename fix",
            &components_type,
            move |components| {
                components.update_typed(&equippable_field, |equippable| {
                    equippable.update_remainder(|tag| tag.rename_field("model", "asset_id"))
                })
            },
        )
    })
}
