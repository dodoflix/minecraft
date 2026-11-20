//! `WallPropertyFix`: turns the boolean `east`/`west`/`north`/`south` properties of wall block states into `low`/`none`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

const WALL_BLOCKS: [&str; 14] = [
    "minecraft:andesite_wall",
    "minecraft:brick_wall",
    "minecraft:cobblestone_wall",
    "minecraft:diorite_wall",
    "minecraft:end_stone_brick_wall",
    "minecraft:granite_wall",
    "minecraft:mossy_cobblestone_wall",
    "minecraft:mossy_stone_brick_wall",
    "minecraft:nether_brick_wall",
    "minecraft:prismarine_wall",
    "minecraft:red_nether_brick_wall",
    "minecraft:red_sandstone_wall",
    "minecraft:sandstone_wall",
    "minecraft:stone_brick_wall",
];

/// `new WallPropertyFix(outputSchema, changesType)`.
pub fn new(schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("WallPropertyFix", schema, changes_type, |df| {
        df.fix_type_everywhere_typed(
            "WallPropertyFix",
            &df.input_schema().get_type(&References::BLOCK_STATE),
            |input| input.update_remainder(upgrade_block_state_tag),
        )
    })
}

fn map_property(value: &str) -> &'static str {
    if value == "true" { "low" } else { "none" }
}

fn fix_wall_property(state: Dynamic, property: &str) -> Dynamic {
    state.update(property, |value| match value.as_string().result() {
        Some(s) => value.create_string(map_property(&s)),
        None => value,
    })
}

fn upgrade_block_state_tag(state: Dynamic) -> Dynamic {
    let is_wall = state
        .get("Name")
        .as_string()
        .result()
        .is_some_and(|name| WALL_BLOCKS.contains(&name.as_str()));
    if !is_wall {
        return state;
    }
    state.update("Properties", |properties| {
        let new_state = fix_wall_property(properties, "east");
        let new_state = fix_wall_property(new_state, "west");
        let new_state = fix_wall_property(new_state, "north");
        fix_wall_property(new_state, "south")
    })
}
