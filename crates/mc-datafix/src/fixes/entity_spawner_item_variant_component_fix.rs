//! `EntitySpawnerItemVariantComponentFix`: variants stored in the `bucket_entity_data` / `entity_data` components of
//! salmon, axolotl and tropical fish buckets and paintings move to their own components.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::extra_data_fix_utils::dye_color_id_to_name;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

/// The `Fixer` interface: `fixRemainder(remainder, bucketData)` of a bucket variant.
type Fixer = fn(Dynamic, Dynamic) -> Dynamic;

/// `new EntitySpawnerItemVariantComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new(
        "EntitySpawnerItemVariantComponentFix",
        output_schema,
        false,
        |df| {
            let item_stack_type = df.input_schema().get_type(&References::ITEM_STACK);
            let id_finder = DSL::field_finder(
                "id",
                &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
            );
            let components_finder = item_stack_type.find_field("components");
            df.fix_type_everywhere_typed(
                "ItemStack bucket_entity_data variants to separate components",
                &item_stack_type,
                move |input| {
                    let id = input
                        .get_optional(&id_finder)
                        .map(|id| id.second().as_str().to_string())
                        .unwrap_or_default();
                    match id.as_str() {
                        "minecraft:salmon_bucket" => input
                            .update_typed(&components_finder, |c| fix_bucket(c, fix_salmon_bucket)),
                        "minecraft:axolotl_bucket" => input.update_typed(&components_finder, |c| {
                            fix_bucket(c, fix_axolotl_bucket)
                        }),
                        "minecraft:tropical_fish_bucket" => input
                            .update_typed(&components_finder, |c| {
                                fix_bucket(c, fix_tropical_fish_bucket)
                            }),
                        "minecraft:painting" => input.update_typed(&components_finder, |c| {
                            write_and_read_typed_or_throw(&c, &c.ty, fix_painting)
                        }),
                        _ => input,
                    }
                },
            )
        },
    )
}

/// `Fixer.apply(components)`: `fixRemainder` on the remainder when it has `minecraft:bucket_entity_data`.
fn fix_bucket(components: Typed, fixer: Fixer) -> Typed {
    components.update_remainder(|remainder| {
        match remainder.get("minecraft:bucket_entity_data").result() {
            Some(bucket_data) => fixer(remainder, bucket_data),
            None => remainder,
        }
    })
}

fn get_base_color(packed_variant: i32) -> &'static str {
    dye_color_id_to_name(packed_variant >> 16 & 0xFF)
}

fn get_pattern_color(packed_variant: i32) -> &'static str {
    dye_color_id_to_name(packed_variant >> 24 & 0xFF)
}

fn get_pattern(packed_variant: i32) -> &'static str {
    match packed_variant & 65535 {
        1 => "flopper",
        256 => "sunstreak",
        257 => "stripey",
        512 => "snooper",
        513 => "glitter",
        768 => "dasher",
        769 => "blockfish",
        1024 => "brinely",
        1025 => "betty",
        1280 => "spotty",
        1281 => "clayfish",
        _ => "kob",
    }
}

fn fix_tropical_fish_bucket(remainder: Dynamic, bucket_data: Dynamic) -> Dynamic {
    let Some(old_variant) = bucket_data.get("BucketVariantTag").as_number().result() else {
        return remainder;
    };
    let packed_variant = old_variant.int_value();
    let pattern = get_pattern(packed_variant);
    let base_color = get_base_color(packed_variant);
    let pattern_color = get_pattern_color(packed_variant);
    remainder
        .update("minecraft:bucket_entity_data", |b| {
            b.remove("BucketVariantTag")
        })
        .set(
            "minecraft:tropical_fish/pattern",
            remainder.create_string(pattern),
        )
        .set(
            "minecraft:tropical_fish/base_color",
            remainder.create_string(base_color),
        )
        .set(
            "minecraft:tropical_fish/pattern_color",
            remainder.create_string(pattern_color),
        )
}

fn fix_axolotl_bucket(remainder: Dynamic, bucket_data: Dynamic) -> Dynamic {
    let Some(old_variant) = bucket_data.get("Variant").as_number().result() else {
        return remainder;
    };
    let new_variant = match old_variant.int_value() {
        1 => "wild",
        2 => "gold",
        3 => "cyan",
        4 => "blue",
        _ => "lucy",
    };
    remainder
        .update("minecraft:bucket_entity_data", |b| b.remove("Variant"))
        .set(
            "minecraft:axolotl/variant",
            remainder.create_string(new_variant),
        )
}

fn fix_salmon_bucket(remainder: Dynamic, bucket_data: Dynamic) -> Dynamic {
    match bucket_data.get("type").result() {
        None => remainder,
        Some(ty) => remainder
            .update("minecraft:bucket_entity_data", |b| b.remove("type"))
            .set("minecraft:salmon/size", ty),
    }
}

fn fix_painting(components: Dynamic) -> Dynamic {
    let Some(entity_data) = components.get("minecraft:entity_data").result() else {
        return components;
    };
    if entity_data.get("id").as_string().result().as_deref() != Some("minecraft:painting") {
        return components;
    }
    let result = entity_data.get("variant").result();
    let entity_data_remainder = entity_data.remove("variant");
    let mut components = if entity_data_remainder.remove("id") == entity_data_remainder.empty_map()
    {
        components.remove("minecraft:entity_data")
    } else {
        components.set("minecraft:entity_data", entity_data_remainder)
    };
    if let Some(result) = result {
        components = components.set("minecraft:painting/variant", result);
    }
    components
}
