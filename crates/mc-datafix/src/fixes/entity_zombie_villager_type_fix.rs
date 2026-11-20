//! `EntityZombieVillagerTypeFix`: converts `IsVillager` zombies to a `ZombieType`.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::random::{RandomSource, create_thread_local_instance_new};
use mc_dfu::Dynamic;

/// `new EntityZombieVillagerTypeFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "EntityZombieVillagerTypeFix",
        output_schema,
        changes_type,
        "EntityZombieVillagerTypeFix",
        &References::ENTITY,
        "Zombie",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(mut input: Dynamic) -> Dynamic {
    if input.get("IsVillager").as_boolean_or(false) {
        if !input.get("ZombieType").is_present() {
            let mut ty = get_villager_profession(input.get("VillagerProfession").as_int_or(-1));
            if ty == -1 {
                ty =
                    get_villager_profession(create_thread_local_instance_new().next_int_bounded(6));
            }
            input = input.set("ZombieType", input.create_int(ty));
        }
        input = input.remove("IsVillager");
    }
    input
}

fn get_villager_profession(profession: i32) -> i32 {
    if (0..6).contains(&profession) {
        profession
    } else {
        -1
    }
}
