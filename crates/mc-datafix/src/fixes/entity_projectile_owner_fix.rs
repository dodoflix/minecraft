//! `EntityProjectileOwnerFix`: moves the old projectile owner UUID encodings (`owner` M/L, `Owner.OwnerUUIDMost/Least`,
//! `OwnerUUIDMost/Least`) into the int-array `OwnerUUID`.

use crate::datafix::DataFix;
use crate::fixes::abstract_uuid_fix::{create_uuid_tag, update_named_choice};
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_dfu::Dynamic;

/// `new EntityProjectileOwnerFix(outputSchema)`.
pub fn new(schema: &Schema) -> DataFix {
    DataFix::new("EntityProjectileOwnerFix", schema, false, |df| {
        let schema = df.input_schema();
        df.fix_type_everywhere_typed(
            "EntityProjectileOwner",
            &schema.get_type(&References::ENTITY),
            move |input| update_projectiles(&schema, input),
        )
    })
}

fn update_projectiles(schema: &Schema, mut input: Typed) -> Typed {
    let update_entity = |input: Typed, name: &str, f: fn(&Dynamic) -> Dynamic| {
        update_named_choice(schema, &References::ENTITY, input, name, |tag| f(&tag))
    };
    for name in [
        "minecraft:egg",
        "minecraft:ender_pearl",
        "minecraft:experience_bottle",
        "minecraft:snowball",
        "minecraft:potion",
    ] {
        input = update_entity(input, name, update_owner_throwable);
    }
    input = update_entity(input, "minecraft:llama_spit", update_owner_llama_spit);
    input = update_entity(input, "minecraft:arrow", update_owner_arrow);
    input = update_entity(input, "minecraft:spectral_arrow", update_owner_arrow);
    update_entity(input, "minecraft:trident", update_owner_arrow)
}

fn update_owner_arrow(tag: &Dynamic) -> Dynamic {
    let most_significant_bits = tag.get("OwnerUUIDMost").as_long_or(0);
    let least_significant_bits = tag.get("OwnerUUIDLeast").as_long_or(0);
    set_uuid(tag, most_significant_bits, least_significant_bits)
        .remove("OwnerUUIDMost")
        .remove("OwnerUUIDLeast")
}

fn update_owner_llama_spit(tag: &Dynamic) -> Dynamic {
    let owner = tag.get("Owner");
    let most_significant_bits = owner.get("OwnerUUIDMost").as_long_or(0);
    let least_significant_bits = owner.get("OwnerUUIDLeast").as_long_or(0);
    set_uuid(tag, most_significant_bits, least_significant_bits).remove("Owner")
}

fn update_owner_throwable(tag: &Dynamic) -> Dynamic {
    let owner = tag.get("owner");
    let most_significant_bits = owner.get("M").as_long_or(0);
    let least_significant_bits = owner.get("L").as_long_or(0);
    set_uuid(tag, most_significant_bits, least_significant_bits).remove("owner")
}

fn set_uuid(tag: &Dynamic, most_significant_bits: i64, least_significant_bits: i64) -> Dynamic {
    if most_significant_bits != 0 && least_significant_bits != 0 {
        tag.set(
            "OwnerUUID",
            create_uuid_tag(tag, most_significant_bits, least_significant_bits),
        )
    } else {
        tag.clone()
    }
}
