//! `TooltipDisplayComponentFix`: folds the `show_in_tooltip` flags and the `hide_tooltip` / `hide_additional_tooltip`
//! components into the `minecraft:tooltip_display` component.

use crate::datafix::DataFix;
use crate::finder::OpticFinder;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::util::write_and_read_typed_or_throw;
use mc_core::java_collections::{HASH_MAP_DEFAULT_CAPACITY, hash_map_ordered};
use mc_core::math::java::java_string_hash;
use mc_dfu::Dynamic;

const CONVERTED_ADDITIONAL_TOOLTIP_TYPES: [&str; 17] = [
    "minecraft:banner_patterns",
    "minecraft:bees",
    "minecraft:block_entity_data",
    "minecraft:block_state",
    "minecraft:bundle_contents",
    "minecraft:charged_projectiles",
    "minecraft:container",
    "minecraft:container_loot",
    "minecraft:firework_explosion",
    "minecraft:fireworks",
    "minecraft:instrument",
    "minecraft:map_id",
    "minecraft:painting/variant",
    "minecraft:pot_decorations",
    "minecraft:potion_contents",
    "minecraft:tropical_fish/pattern",
    "minecraft:written_book_content",
];

/// The `HashSet<String>` of hidden component ids: distinct entries in insertion order (see [`hidden_in_set_order`]).
type HiddenTooltips = Vec<String>;

/// `hiddenTooltips.add(componentId)`.
fn add(hidden_tooltips: &mut HiddenTooltips, component_id: &str) {
    if !hidden_tooltips.iter().any(|id| id == component_id) {
        hidden_tooltips.push(component_id.to_string());
    }
}

/// `hiddenTooltips.stream()`: the `HashSet` iteration order.
fn hidden_in_set_order(hidden_tooltips: HiddenTooltips) -> Vec<String> {
    hash_map_ordered(
        hidden_tooltips,
        |id| java_string_hash(id),
        HASH_MAP_DEFAULT_CAPACITY,
    )
}

/// `new TooltipDisplayComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("TooltipDisplayComponentFix", output_schema, true, |df| {
        let components_type = df.input_schema().get_type(&References::DATA_COMPONENTS);
        let new_components_type = df.output_schema().get_type(&References::DATA_COMPONENTS);
        let can_place_on_finder = components_type.find_field("minecraft:can_place_on");
        let can_break_finder = components_type.find_field("minecraft:can_break");
        let new_can_place_on_type = new_components_type.find_field_type("minecraft:can_place_on");
        let new_can_break_type = new_components_type.find_field_type("minecraft:can_break");
        df.fix_type_everywhere_typed_new(
            "TooltipDisplayComponentFix",
            &components_type,
            &new_components_type,
            move |typed| {
                fix(
                    typed,
                    &can_place_on_finder,
                    &can_break_finder,
                    &new_can_place_on_type,
                    &new_can_break_type,
                )
            },
        )
    })
}

fn fix(
    typed: Typed,
    can_place_on_finder: &OpticFinder,
    can_break_finder: &OpticFinder,
    new_can_place_on_type: &Type,
    new_can_break_type: &Type,
) -> Typed {
    let mut hidden_tooltips: HiddenTooltips = Vec::new();
    let typed = fix_adventure_mode_predicate(
        typed,
        can_place_on_finder,
        new_can_place_on_type,
        "minecraft:can_place_on",
        &mut hidden_tooltips,
    );
    let typed = fix_adventure_mode_predicate(
        typed,
        can_break_finder,
        new_can_break_type,
        "minecraft:can_break",
        &mut hidden_tooltips,
    );
    typed.update_remainder(|remainder| {
        let remainder = fix_simple_component(remainder, "minecraft:trim", &mut hidden_tooltips);
        let remainder =
            fix_simple_component(remainder, "minecraft:unbreakable", &mut hidden_tooltips);
        let remainder = fix_component_and_unwrap(
            remainder,
            "minecraft:dyed_color",
            "rgb",
            &mut hidden_tooltips,
        );
        let remainder = fix_component_and_unwrap(
            remainder,
            "minecraft:attribute_modifiers",
            "modifiers",
            &mut hidden_tooltips,
        );
        let remainder = fix_component_and_unwrap(
            remainder,
            "minecraft:enchantments",
            "levels",
            &mut hidden_tooltips,
        );
        let remainder = fix_component_and_unwrap(
            remainder,
            "minecraft:stored_enchantments",
            "levels",
            &mut hidden_tooltips,
        );
        let remainder = fix_component_and_unwrap(
            remainder,
            "minecraft:jukebox_playable",
            "song",
            &mut hidden_tooltips,
        );
        let hide_tooltip = remainder.get("minecraft:hide_tooltip").is_present();
        let remainder = remainder.remove("minecraft:hide_tooltip");
        let hide_additional_tooltip = remainder
            .get("minecraft:hide_additional_tooltip")
            .is_present();
        let remainder = remainder.remove("minecraft:hide_additional_tooltip");
        if hide_additional_tooltip {
            for component_id in CONVERTED_ADDITIONAL_TOOLTIP_TYPES {
                if remainder.get(component_id).is_present() {
                    add(&mut hidden_tooltips, component_id);
                }
            }
        }
        if hidden_tooltips.is_empty() && !hide_tooltip {
            remainder
        } else {
            let tooltip_display = remainder.create_map([
                (
                    remainder.create_string("hide_tooltip"),
                    remainder.create_boolean(hide_tooltip),
                ),
                (
                    remainder.create_string("hidden_components"),
                    remainder.create_list(
                        hidden_in_set_order(std::mem::take(&mut hidden_tooltips))
                            .into_iter()
                            .map(|id| remainder.create_string(id)),
                    ),
                ),
            ]);
            remainder.set("minecraft:tooltip_display", tooltip_display)
        }
    })
}

fn fix_simple_component(
    remainder: Dynamic,
    component_id: &str,
    hidden_tooltips: &mut HiddenTooltips,
) -> Dynamic {
    fix_remainder_component(remainder, component_id, hidden_tooltips, |component| {
        component
    })
}

fn fix_component_and_unwrap(
    remainder: Dynamic,
    component_id: &str,
    field_name: &str,
    hidden_tooltips: &mut HiddenTooltips,
) -> Dynamic {
    fix_remainder_component(remainder, component_id, hidden_tooltips, |component| {
        component.get(field_name).result().unwrap_or(component)
    })
}

fn fix_remainder_component(
    remainder: Dynamic,
    component_id: &str,
    hidden_tooltips: &mut HiddenTooltips,
    fixer: impl FnOnce(Dynamic) -> Dynamic,
) -> Dynamic {
    remainder.update(component_id, |component| {
        let show_in_tooltip = component.get("show_in_tooltip").as_boolean_or(true);
        if !show_in_tooltip {
            add(hidden_tooltips, component_id);
        }
        fixer(component.remove("show_in_tooltip"))
    })
}

fn fix_adventure_mode_predicate(
    typed_components: Typed,
    component_finder: &OpticFinder,
    new_type: &Type,
    component_id: &str,
    hidden_tooltips: &mut HiddenTooltips,
) -> Typed {
    typed_components.update_typed_as(component_finder, new_type, |typed_component| {
        write_and_read_typed_or_throw(&typed_component, new_type, |component| {
            let Some(predicates) = component.get("predicates").result() else {
                return component;
            };
            let show_in_tooltip = component.get("show_in_tooltip").as_boolean_or(true);
            if !show_in_tooltip {
                add(hidden_tooltips, component_id);
            }
            predicates
        })
    })
}
