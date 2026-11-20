//! `AreaEffectCloudPotionFix`: moves the color, effects and potion of an area effect cloud into `potion_contents`.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new AreaEffectCloudPotionFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "AreaEffectCloudPotionFix",
        output_schema,
        false,
        "AreaEffectCloudPotionFix",
        &References::ENTITY,
        "minecraft:area_effect_cloud",
        |entity| entity.update_remainder(fix),
    )
}

fn fix(entity: Dynamic) -> Dynamic {
    let color = entity.get("Color").result();
    let effects = entity.get("effects").result();
    let potion = entity.get("Potion").result();
    let entity = entity.remove("Color").remove("effects").remove("Potion");
    if color.is_none() && effects.is_none() && potion.is_none() {
        return entity;
    }
    let mut potion_contents = entity.empty_map();
    if let Some(color) = color {
        potion_contents = potion_contents.set("custom_color", color);
    }
    if let Some(effects) = effects {
        potion_contents = potion_contents.set("custom_effects", effects);
    }
    if let Some(potion) = potion {
        potion_contents = potion_contents.set("potion", potion);
    }
    entity.set("potion_contents", potion_contents)
}
