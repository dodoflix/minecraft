//! `EffectDurationFix`: replaces the effect change timestamp of the factor calculation data by the ticks the effect was
//! active.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use mc_dfu::Dynamic;

const POTION_ITEMS: [&str; 4] = [
    "minecraft:potion",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:tipped_arrow",
];

/// `new EffectDurationFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("EffectDurationFix", output_schema, false, |df| {
        let input_schema = df.input_schema();
        let item_stack_type = input_schema.get_type(&References::ITEM_STACK);
        let id_finder = DSL::field_finder(
            "id",
            &DSL::named_type(References::ITEM_NAME.type_name(), &DSL::namespaced_string()),
        );
        let tag_finder = item_stack_type.find_field("tag");
        TypeRewriteRule::seq_all(
            df.fix_type_everywhere_typed(
                "EffectDurationEntity",
                &input_schema.get_type(&References::ENTITY),
                |input| input.update_remainder(update_entity),
            ),
            vec![
                df.fix_type_everywhere_typed(
                    "EffectDurationPlayer",
                    &input_schema.get_type(&References::PLAYER),
                    |input| input.update_remainder(update_entity),
                ),
                df.fix_type_everywhere_typed(
                    "EffectDurationItem",
                    &item_stack_type,
                    move |input| {
                        let is_potion = input
                            .get_optional(&id_finder)
                            .is_some_and(|id| POTION_ITEMS.contains(&id.second().as_str()));
                        if is_potion && let Some(tag) = input.get_optional_typed(&tag_finder) {
                            let tag_rest = tag.get_remainder();
                            let new_tag = tag
                                .set_remainder(tag_rest.update("CustomPotionEffects", fix_effects));
                            return input.set_typed(&tag_finder, new_tag);
                        }
                        input
                    },
                ),
            ],
        )
    })
}

fn fix_effect(effect: Dynamic) -> Dynamic {
    let source = effect.clone();
    effect.update("FactorCalculationData", move |factor_data| {
        let timestamp = factor_data.get("effect_changed_timestamp").as_int_or(-1);
        let factor_data = factor_data.remove("effect_changed_timestamp");
        let duration = source.get("Duration").as_int_or(-1);
        let ticks_active = timestamp.wrapping_sub(duration);
        factor_data.set("ticks_active", factor_data.create_int(ticks_active))
    })
}

fn fix_effects(input: Dynamic) -> Dynamic {
    let effects = input
        .as_stream()
        .into_iter()
        .map(fix_effect)
        .collect::<Vec<_>>();
    input.create_list(effects)
}

fn update_entity(data: Dynamic) -> Dynamic {
    let data = data.update("Effects", fix_effects);
    let data = data.update("ActiveEffects", fix_effects);
    data.update("CustomPotionEffects", fix_effects)
}
