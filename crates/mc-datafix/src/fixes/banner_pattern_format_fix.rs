//! `BannerPatternFormatFix`: banner patterns get namespaced ids and a dye color name.

use crate::datafix::DataFix;
use crate::extra_data_fix_utils::dye_color_id_to_name;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

const PATTERN_ID_MAP: [(&str, &str); 41] = [
    ("b", "minecraft:base"),
    ("bl", "minecraft:square_bottom_left"),
    ("br", "minecraft:square_bottom_right"),
    ("tl", "minecraft:square_top_left"),
    ("tr", "minecraft:square_top_right"),
    ("bs", "minecraft:stripe_bottom"),
    ("ts", "minecraft:stripe_top"),
    ("ls", "minecraft:stripe_left"),
    ("rs", "minecraft:stripe_right"),
    ("cs", "minecraft:stripe_center"),
    ("ms", "minecraft:stripe_middle"),
    ("drs", "minecraft:stripe_downright"),
    ("dls", "minecraft:stripe_downleft"),
    ("ss", "minecraft:small_stripes"),
    ("cr", "minecraft:cross"),
    ("sc", "minecraft:straight_cross"),
    ("bt", "minecraft:triangle_bottom"),
    ("tt", "minecraft:triangle_top"),
    ("bts", "minecraft:triangles_bottom"),
    ("tts", "minecraft:triangles_top"),
    ("ld", "minecraft:diagonal_left"),
    ("rd", "minecraft:diagonal_up_right"),
    ("lud", "minecraft:diagonal_up_left"),
    ("rud", "minecraft:diagonal_right"),
    ("mc", "minecraft:circle"),
    ("mr", "minecraft:rhombus"),
    ("vh", "minecraft:half_vertical"),
    ("hh", "minecraft:half_horizontal"),
    ("vhr", "minecraft:half_vertical_right"),
    ("hhb", "minecraft:half_horizontal_bottom"),
    ("bo", "minecraft:border"),
    ("cbo", "minecraft:curly_border"),
    ("gra", "minecraft:gradient"),
    ("gru", "minecraft:gradient_up"),
    ("bri", "minecraft:bricks"),
    ("glb", "minecraft:globe"),
    ("cre", "minecraft:creeper"),
    ("sku", "minecraft:skull"),
    ("flo", "minecraft:flower"),
    ("moj", "minecraft:mojang"),
    ("pig", "minecraft:piglin"),
];

/// `new BannerPatternFormatFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "BannerPatternFormatFix",
        output_schema,
        false,
        "BannerPatternFormatFix",
        &References::BLOCK_ENTITY,
        "minecraft:banner",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(tag: Dynamic) -> Dynamic {
    tag.rename_and_fix_field("Patterns", "patterns", |patterns| {
        let layers: Vec<Dynamic> = patterns.as_stream().into_iter().map(fix_layer).collect();
        patterns.create_list(layers)
    })
}

fn fix_layer(dynamic: Dynamic) -> Dynamic {
    let dynamic = dynamic.rename_and_fix_field("Pattern", "pattern", |pattern| {
        match pattern.as_string().result() {
            Some(id) => {
                let mapped = PATTERN_ID_MAP
                    .iter()
                    .find(|(short, _)| *short == id)
                    .map_or(id.as_lossy_str(), |(_, long)| long);
                pattern.create_string(mapped)
            }
            None => pattern,
        }
    });
    let color = dynamic.create_string(dye_color_id_to_name(dynamic.get("Color").as_int_or(0)));
    dynamic.set("color", color).remove("Color")
}
