//! `SavedDataUUIDFix`: converts the `HeroesOfTheVillage` UUIDs of the raids saved data to int arrays.

use crate::datafix::DataFix;
use crate::fixes::abstract_uuid_fix::create_uuid_from_longs;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new SavedDataUUIDFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("SavedDataUUIDFix", schema, false, |df| {
        df.fix_type_everywhere_typed(
            "SavedDataUUIDFix",
            &df.input_schema().get_type(&References::SAVED_DATA_RAIDS),
            |input| {
                input.update_remainder(|container| {
                    container.update("data", |tag| {
                        tag.update("Raids", |raids| {
                            raids.create_list(raids.as_stream().into_iter().map(|raid| {
                                raid.update("HeroesOfTheVillage", |heros| {
                                    heros.create_list(heros.as_stream().into_iter().map(|hero| {
                                        create_uuid_from_longs(&hero, "UUIDMost", "UUIDLeast")
                                            .unwrap_or_else(|| {
                                                log::warn!(
                                                    "HeroesOfTheVillage contained invalid UUIDs."
                                                );
                                                hero
                                            })
                                    }))
                                })
                            }))
                        })
                    })
                })
            },
        )
    })
}
