//! `OminousBannerRenameFix`: renames the illager banner translation key in the `display.Name` of white banners.

use crate::datafix::DataFix;
use crate::fixes::item_stack_tag_fix;
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;

/// `new OminousBannerRenameFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    item_stack_tag_fix::new(
        "OminousBannerRenameFix",
        output_schema,
        "OminousBannerRenameFix",
        |id| id == "minecraft:white_banner",
        |tag| write_and_read_typed_or_throw(&tag, &tag.ty, fix_item_stack_tag),
    )
}

/// `OminousBannerRenameFix.fixItemStackTag(Dynamic)`.
fn fix_item_stack_tag(tag: Dynamic) -> Dynamic {
    tag.update("display", |display| {
        display.update("Name", |name| match name.as_string().result() {
            Some(string) => name.create_string(string.map_lossless(|t| {
                t.replace(
                    "\"translate\":\"block.minecraft.illager_banner\"",
                    "\"translate\":\"block.minecraft.ominous_banner\"",
                )
            })),
            None => name,
        })
    })
}
