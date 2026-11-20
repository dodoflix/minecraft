//! `BlockEntityFurnaceBurnTimeFix`: renames the cook/burn time fields of a furnace-like block entity and copies the
//! remaining burn time into `lit_total_time`.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new BlockEntityFurnaceBurnTimeFix(outputSchema, entityType)`.
pub fn new(output_schema: &Schema, entity_type: &str) -> DataFix {
    named_entity_fix::new(
        "BlockEntityFurnaceBurnTimeFix",
        output_schema,
        false,
        &format!("BlockEntityFurnaceBurnTimeFix{entity_type}"),
        &References::BLOCK_ENTITY,
        entity_type,
        |entity| entity.update_remainder(fix_burn_time),
    )
}

/// `fixBurnTime(data)`.
pub fn fix_burn_time(data: Dynamic) -> Dynamic {
    let data = data.rename_field("CookTime", "cooking_time_spent");
    let data = data.rename_field("CookTimeTotal", "cooking_total_time");
    let data = data.rename_field("BurnTime", "lit_time_remaining");
    let lit_time_remaining = data.get("lit_time_remaining").result();
    data.set_field_if_present("lit_total_time", lit_time_remaining)
}
