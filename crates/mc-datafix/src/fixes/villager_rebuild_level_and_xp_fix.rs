//! `VillagerRebuildLevelAndXpFix`: derives the `level` and `Xp` of villagers from their trade offers.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::finder::remainder_finder;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;

const LEVEL_XP_THRESHOLDS: [i32; 5] = [0, 10, 50, 100, 150];

/// `VillagerRebuildLevelAndXpFix.getMinXpPerLevel(level)`.
pub fn get_min_xp_per_level(level: i32) -> i32 {
    LEVEL_XP_THRESHOLDS[level
        .wrapping_sub(1)
        .clamp(0, LEVEL_XP_THRESHOLDS.len() as i32 - 1) as usize]
}

/// `new VillagerRebuildLevelAndXpFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "VillagerRebuildLevelAndXpFix",
        output_schema,
        changes_type,
        |df| {
            let villager_type = df
                .input_schema()
                .get_choice_type(&References::ENTITY, "minecraft:villager");
            let entity_f = DSL::named_choice("minecraft:villager", &villager_type);
            let offers_f = villager_type.find_field("Offers");
            let offers_type = offers_f.ty().clone();
            let recipe_list_f = offers_type.find_field("Recipes");
            let recipe_f = recipe_list_f
                .ty()
                .as_list()
                .expect("java.lang.ClassCastException: Recipes is not a list type")
                .finder();
            df.fix_type_everywhere_typed(
                "Villager level and xp rebuild",
                &df.input_schema().get_type(&References::ENTITY),
                move |input| {
                    input.update_typed_as(&entity_f, &villager_type, |villager| {
                        let remainder = villager.get(&remainder_finder()).into_dynamic();
                        let mut level = remainder.get("VillagerData").get("level").as_int_or(0);
                        let mut modified_villager = villager;
                        if level == 0 || level == 1 {
                            let offer_count = modified_villager
                                .get_optional_typed(&offers_f)
                                .and_then(|o| o.get_optional_typed(&recipe_list_f))
                                .map_or(0, |recipe_list| {
                                    recipe_list.get_all_typed(&recipe_f).len() as i32
                                });
                            level = (offer_count / 2).clamp(1, 5);
                            if level > 1 {
                                modified_villager = add_level(modified_villager, level);
                            }
                        }
                        if remainder.get("Xp").as_number().result().is_none() {
                            modified_villager = add_xp_from_level(modified_villager, level);
                        }
                        modified_villager
                    })
                },
            )
        },
    )
}

/// `VillagerRebuildLevelAndXpFix.addLevel(villager, level)`.
fn add_level(villager: Typed, level: i32) -> Typed {
    villager.update_remainder(|remainder| {
        remainder.update("VillagerData", |villager_data| {
            villager_data.set("level", villager_data.create_int(level))
        })
    })
}

/// `VillagerRebuildLevelAndXpFix.addXpFromLevel(villager, level)`.
fn add_xp_from_level(villager: Typed, level: i32) -> Typed {
    let xp = get_min_xp_per_level(level);
    villager.update_remainder(|remainder| remainder.set("Xp", remainder.create_int(xp)))
}
