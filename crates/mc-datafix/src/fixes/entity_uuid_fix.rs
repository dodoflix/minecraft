//! `EntityUUIDFix`: converts the `UUID`, `OwnerUUID`, `LoveCause`, attribute modifier, leash and similar fields of the
//! entities to int-array UUIDs.

use crate::datafix::DataFix;
use crate::fixes::abstract_uuid_fix::{
    create_uuid_from_ml, replace_uuid_least_most, replace_uuid_ml_tag, replace_uuid_string,
    update_named_choice,
};
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;

// Java iterates `HashSet`s; each call only touches its own choice, so the order inside a set is not observable.
const ABSTRACT_HORSES: &[&str] = &[
    "minecraft:donkey",
    "minecraft:horse",
    "minecraft:llama",
    "minecraft:mule",
    "minecraft:skeleton_horse",
    "minecraft:trader_llama",
    "minecraft:zombie_horse",
];
const TAMEABLE_ANIMALS: &[&str] = &["minecraft:cat", "minecraft:parrot", "minecraft:wolf"];
const ANIMALS: &[&str] = &[
    "minecraft:bee",
    "minecraft:chicken",
    "minecraft:cow",
    "minecraft:fox",
    "minecraft:mooshroom",
    "minecraft:ocelot",
    "minecraft:panda",
    "minecraft:pig",
    "minecraft:polar_bear",
    "minecraft:rabbit",
    "minecraft:sheep",
    "minecraft:turtle",
    "minecraft:hoglin",
];
const MOBS: &[&str] = &[
    "minecraft:bat",
    "minecraft:blaze",
    "minecraft:cave_spider",
    "minecraft:cod",
    "minecraft:creeper",
    "minecraft:dolphin",
    "minecraft:drowned",
    "minecraft:elder_guardian",
    "minecraft:ender_dragon",
    "minecraft:enderman",
    "minecraft:endermite",
    "minecraft:evoker",
    "minecraft:ghast",
    "minecraft:giant",
    "minecraft:guardian",
    "minecraft:husk",
    "minecraft:illusioner",
    "minecraft:magma_cube",
    "minecraft:pufferfish",
    "minecraft:zombified_piglin",
    "minecraft:salmon",
    "minecraft:shulker",
    "minecraft:silverfish",
    "minecraft:skeleton",
    "minecraft:slime",
    "minecraft:snow_golem",
    "minecraft:spider",
    "minecraft:squid",
    "minecraft:stray",
    "minecraft:tropical_fish",
    "minecraft:vex",
    "minecraft:villager",
    "minecraft:iron_golem",
    "minecraft:vindicator",
    "minecraft:pillager",
    "minecraft:wandering_trader",
    "minecraft:witch",
    "minecraft:wither",
    "minecraft:wither_skeleton",
    "minecraft:zombie",
    "minecraft:zombie_villager",
    "minecraft:phantom",
    "minecraft:ravager",
    "minecraft:piglin",
];
const LIVING_ENTITIES: &[&str] = &["minecraft:armor_stand"];
const PROJECTILES: &[&str] = &[
    "minecraft:arrow",
    "minecraft:dragon_fireball",
    "minecraft:firework_rocket",
    "minecraft:fireball",
    "minecraft:llama_spit",
    "minecraft:small_fireball",
    "minecraft:snowball",
    "minecraft:spectral_arrow",
    "minecraft:egg",
    "minecraft:ender_pearl",
    "minecraft:experience_bottle",
    "minecraft:potion",
    "minecraft:trident",
    "minecraft:wither_skull",
];

/// `new EntityUUIDFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("EntityUUIDFix", schema, false, |df| {
        let schema = df.input_schema();
        df.fix_type_everywhere_typed(
            "EntityUUIDFixes",
            &schema.get_type(&References::ENTITY),
            move |input| {
                let choice = |input: Typed, name: &str, f: fn(&Dynamic) -> Dynamic| {
                    update_named_choice(&schema, &References::ENTITY, input, name, |tag| f(&tag))
                };
                let mut input = input.update_remainder(|tag| update_entity_uuid(&tag));
                for &name in ABSTRACT_HORSES {
                    input = choice(input, name, update_animal_owner);
                }
                for &name in TAMEABLE_ANIMALS {
                    input = choice(input, name, update_animal_owner);
                }
                for &name in ANIMALS {
                    input = choice(input, name, update_animal);
                }
                for &name in MOBS {
                    input = choice(input, name, update_mob);
                }
                for &name in LIVING_ENTITIES {
                    input = choice(input, name, update_living_entity);
                }
                for &name in PROJECTILES {
                    input = choice(input, name, update_projectile);
                }
                input = choice(input, "minecraft:bee", update_hurt_by);
                input = choice(input, "minecraft:zombified_piglin", update_hurt_by);
                input = choice(input, "minecraft:fox", update_fox);
                input = choice(input, "minecraft:item", update_item);
                input = choice(input, "minecraft:shulker_bullet", update_shulker_bullet);
                input = choice(
                    input,
                    "minecraft:area_effect_cloud",
                    update_area_effect_cloud,
                );
                input = choice(input, "minecraft:zombie_villager", update_zombie_villager);
                input = choice(input, "minecraft:evoker_fangs", update_evoker_fangs);
                choice(input, "minecraft:piglin", update_piglin)
            },
        )
    })
}

fn update_piglin(tag: &Dynamic) -> Dynamic {
    tag.update("Brain", |brain| {
        brain.update("memories", |memories| {
            memories.update("minecraft:angry_at", |angry_at| {
                replace_uuid_string(&angry_at, "value", "value").unwrap_or_else(|| {
                    log::warn!("angry_at has no value.");
                    angry_at
                })
            })
        })
    })
}

fn update_evoker_fangs(tag: &Dynamic) -> Dynamic {
    replace_uuid_least_most(tag, "OwnerUUID", "Owner").unwrap_or_else(|| tag.clone())
}

fn update_zombie_villager(tag: &Dynamic) -> Dynamic {
    replace_uuid_least_most(tag, "ConversionPlayer", "ConversionPlayer")
        .unwrap_or_else(|| tag.clone())
}

fn update_area_effect_cloud(tag: &Dynamic) -> Dynamic {
    replace_uuid_least_most(tag, "OwnerUUID", "Owner").unwrap_or_else(|| tag.clone())
}

fn update_shulker_bullet(tag: &Dynamic) -> Dynamic {
    let tag = replace_uuid_ml_tag(tag, "Owner", "Owner").unwrap_or_else(|| tag.clone());
    replace_uuid_ml_tag(&tag, "Target", "Target").unwrap_or(tag)
}

fn update_item(tag: &Dynamic) -> Dynamic {
    let tag = replace_uuid_ml_tag(tag, "Owner", "Owner").unwrap_or_else(|| tag.clone());
    replace_uuid_ml_tag(&tag, "Thrower", "Thrower").unwrap_or(tag)
}

fn update_fox(tag: &Dynamic) -> Dynamic {
    let trusted_uuids = tag.get("TrustedUUIDs").result().map(|uuid_tags| {
        tag.create_list(uuid_tags.as_stream().into_iter().map(|uuid_tag| {
            create_uuid_from_ml(&uuid_tag).unwrap_or_else(|| {
                log::warn!("Trusted contained invalid data.");
                uuid_tag
            })
        }))
    });
    match trusted_uuids {
        Some(trusted) => tag.remove("TrustedUUIDs").set("Trusted", trusted),
        None => tag.clone(),
    }
}

fn update_hurt_by(tag: &Dynamic) -> Dynamic {
    replace_uuid_string(tag, "HurtBy", "HurtBy").unwrap_or_else(|| tag.clone())
}

fn update_animal_owner(tag: &Dynamic) -> Dynamic {
    let fixed = update_animal(tag);
    replace_uuid_string(&fixed, "OwnerUUID", "Owner").unwrap_or(fixed)
}

fn update_animal(tag: &Dynamic) -> Dynamic {
    let fixed = update_mob(tag);
    replace_uuid_least_most(&fixed, "LoveCause", "LoveCause").unwrap_or(fixed)
}

fn update_mob(tag: &Dynamic) -> Dynamic {
    update_living_entity(tag).update("Leash", |leash_tag| {
        replace_uuid_least_most(&leash_tag, "UUID", "UUID").unwrap_or(leash_tag)
    })
}

/// `updateLivingEntity(tag)`.
pub fn update_living_entity(tag: &Dynamic) -> Dynamic {
    tag.update("Attributes", |attributes| {
        tag.create_list(attributes.as_stream().into_iter().map(|attribute| {
            attribute.update("Modifiers", |modifiers| {
                attribute.create_list(modifiers.as_stream().into_iter().map(|modifier| {
                    replace_uuid_least_most(&modifier, "UUID", "UUID").unwrap_or(modifier)
                }))
            })
        }))
    })
}

fn update_projectile(tag: &Dynamic) -> Dynamic {
    match tag.get("OwnerUUID").result() {
        Some(owner) => tag.remove("OwnerUUID").set("Owner", owner),
        None => tag.clone(),
    }
}

/// `updateEntityUUID(tag)`.
pub fn update_entity_uuid(tag: &Dynamic) -> Dynamic {
    replace_uuid_least_most(tag, "UUID", "UUID").unwrap_or_else(|| tag.clone())
}
