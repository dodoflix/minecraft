//! `LevelUUIDFix`: converts the dragon fight, wandering trader and custom boss event player UUIDs of the level.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::abstract_uuid_fix::{
    create_uuid_from_ml, replace_uuid_least_most, replace_uuid_string,
};
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new LevelUUIDFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("LevelUUIDFix", schema, false, |df| {
        let input_schema = df.input_schema();
        let ty = input_schema.get_type(&References::LEVEL);
        let custom_boss_events_f = ty.find_field("CustomBossEvents");
        let custom_boss_event_f = DSL::type_finder(&DSL::and_type(
            &DSL::optional_type(&DSL::field_type(
                "Name",
                &input_schema.get_type_raw(&References::TEXT_COMPONENT),
            )),
            &DSL::remainder_type(),
        ));
        df.fix_type_everywhere_typed("LevelUUIDFix", &ty, move |input| {
            input
                .update_remainder(|tag| {
                    let tag = update_dragon_fight(&tag);
                    update_wandering_trader(&tag)
                })
                .update_typed(&custom_boss_events_f, |custom_boss_events| {
                    custom_boss_events.update_typed(&custom_boss_event_f, |event| {
                        event.update_remainder(|tag| update_custom_boss_event(&tag))
                    })
                })
        })
    })
}

fn update_wandering_trader(tag: &Dynamic) -> Dynamic {
    replace_uuid_string(tag, "WanderingTraderId", "WanderingTraderId")
        .unwrap_or_else(|| tag.clone())
}

/// `updateDragonFight(tag)`.
pub fn update_dragon_fight(tag: &Dynamic) -> Dynamic {
    tag.update("DimensionData", |dimension_data_map| {
        dimension_data_map.update_map_values(|(key, dimension_data)| {
            let dimension_data = dimension_data.update("DragonFight", |dragonfight| {
                replace_uuid_least_most(&dragonfight, "DragonUUID", "Dragon").unwrap_or(dragonfight)
            });
            (key, dimension_data)
        })
    })
}

fn update_custom_boss_event(tag: &Dynamic) -> Dynamic {
    tag.update("Players", |players| {
        tag.create_list(players.as_stream().into_iter().map(|player| {
            create_uuid_from_ml(&player).unwrap_or_else(|| {
                log::warn!("CustomBossEvents contains invalid UUIDs.");
                player
            })
        }))
    })
}
