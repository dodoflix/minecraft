//! `RaidRenamesDataFix`: renames the fields of the raids saved data to snake case.

use crate::datafix::DataFix;
use crate::extra_data_fix_utils::fix_inline_block_pos;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new RaidRenamesDataFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("RaidRenamesDataFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "RaidRenamesDataFix",
            &df.input_schema().get_type(&References::SAVED_DATA_RAIDS),
            |input| input.update_remainder(|container| container.update("data", fix)),
        )
    })
}

fn fix(tag: Dynamic) -> Dynamic {
    tag.rename_and_fix_field("Raids", "raids", |raids| {
        raids.create_list(raids.as_stream().into_iter().map(|raid| fix_raid(&raid)))
    })
    .rename_field("Tick", "tick")
    .rename_field("NextAvailableID", "next_id")
}

fn fix_raid(raid: &Dynamic) -> Dynamic {
    fix_inline_block_pos(raid, "CX", "CY", "CZ", "center")
        .rename_field("Id", "id")
        .rename_field("Started", "started")
        .rename_field("Active", "active")
        .rename_field("TicksActive", "ticks_active")
        .rename_field("BadOmenLevel", "raid_omen_level")
        .rename_field("GroupsSpawned", "groups_spawned")
        .rename_field("PreRaidTicks", "cooldown_ticks")
        .rename_field("PostRaidTicks", "post_raid_ticks")
        .rename_field("TotalHealth", "total_health")
        .rename_field("NumGroups", "group_count")
        .rename_field("Status", "status")
        .rename_field("HeroesOfTheVillage", "heroes_of_the_village")
}
