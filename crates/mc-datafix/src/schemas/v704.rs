//! `net.minecraft.util.datafix.schemas.V704`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::ty::HookFunction;
use mc_dfu::Dynamic;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::dsl::TypeReference;
use crate::fixes::block_entity_id_fix;
use crate::schemas::v99::{self, IdMap};

/// `V704.ITEM_TO_BLOCKENTITY`.
pub(crate) static ITEM_TO_BLOCKENTITY: LazyLock<IdMap> = LazyLock::new(|| {
    HashMap::from([
        ("minecraft:furnace", "minecraft:furnace"),
        ("minecraft:lit_furnace", "minecraft:furnace"),
        ("minecraft:chest", "minecraft:chest"),
        ("minecraft:trapped_chest", "minecraft:chest"),
        ("minecraft:ender_chest", "minecraft:ender_chest"),
        ("minecraft:jukebox", "minecraft:jukebox"),
        ("minecraft:dispenser", "minecraft:dispenser"),
        ("minecraft:dropper", "minecraft:dropper"),
        ("minecraft:sign", "minecraft:sign"),
        ("minecraft:mob_spawner", "minecraft:mob_spawner"),
        ("minecraft:spawner", "minecraft:mob_spawner"),
        ("minecraft:noteblock", "minecraft:noteblock"),
        ("minecraft:brewing_stand", "minecraft:brewing_stand"),
        ("minecraft:enhanting_table", "minecraft:enchanting_table"),
        ("minecraft:command_block", "minecraft:command_block"),
        ("minecraft:beacon", "minecraft:beacon"),
        ("minecraft:skull", "minecraft:skull"),
        ("minecraft:daylight_detector", "minecraft:daylight_detector"),
        ("minecraft:hopper", "minecraft:hopper"),
        ("minecraft:banner", "minecraft:banner"),
        ("minecraft:flower_pot", "minecraft:flower_pot"),
        (
            "minecraft:repeating_command_block",
            "minecraft:command_block",
        ),
        ("minecraft:chain_command_block", "minecraft:command_block"),
        ("minecraft:shulker_box", "minecraft:shulker_box"),
        ("minecraft:white_shulker_box", "minecraft:shulker_box"),
        ("minecraft:orange_shulker_box", "minecraft:shulker_box"),
        ("minecraft:magenta_shulker_box", "minecraft:shulker_box"),
        ("minecraft:light_blue_shulker_box", "minecraft:shulker_box"),
        ("minecraft:yellow_shulker_box", "minecraft:shulker_box"),
        ("minecraft:lime_shulker_box", "minecraft:shulker_box"),
        ("minecraft:pink_shulker_box", "minecraft:shulker_box"),
        ("minecraft:gray_shulker_box", "minecraft:shulker_box"),
        ("minecraft:silver_shulker_box", "minecraft:shulker_box"),
        ("minecraft:cyan_shulker_box", "minecraft:shulker_box"),
        ("minecraft:purple_shulker_box", "minecraft:shulker_box"),
        ("minecraft:blue_shulker_box", "minecraft:shulker_box"),
        ("minecraft:brown_shulker_box", "minecraft:shulker_box"),
        ("minecraft:green_shulker_box", "minecraft:shulker_box"),
        ("minecraft:red_shulker_box", "minecraft:shulker_box"),
        ("minecraft:black_shulker_box", "minecraft:shulker_box"),
        ("minecraft:bed", "minecraft:bed"),
        ("minecraft:light_gray_shulker_box", "minecraft:shulker_box"),
        ("minecraft:banner", "minecraft:banner"),
        ("minecraft:white_banner", "minecraft:banner"),
        ("minecraft:orange_banner", "minecraft:banner"),
        ("minecraft:magenta_banner", "minecraft:banner"),
        ("minecraft:light_blue_banner", "minecraft:banner"),
        ("minecraft:yellow_banner", "minecraft:banner"),
        ("minecraft:lime_banner", "minecraft:banner"),
        ("minecraft:pink_banner", "minecraft:banner"),
        ("minecraft:gray_banner", "minecraft:banner"),
        ("minecraft:silver_banner", "minecraft:banner"),
        ("minecraft:light_gray_banner", "minecraft:banner"),
        ("minecraft:cyan_banner", "minecraft:banner"),
        ("minecraft:purple_banner", "minecraft:banner"),
        ("minecraft:blue_banner", "minecraft:banner"),
        ("minecraft:brown_banner", "minecraft:banner"),
        ("minecraft:green_banner", "minecraft:banner"),
        ("minecraft:red_banner", "minecraft:banner"),
        ("minecraft:black_banner", "minecraft:banner"),
        ("minecraft:standing_sign", "minecraft:sign"),
        ("minecraft:wall_sign", "minecraft:sign"),
        ("minecraft:piston_head", "minecraft:piston"),
        (
            "minecraft:daylight_detector_inverted",
            "minecraft:daylight_detector",
        ),
        ("minecraft:unpowered_comparator", "minecraft:comparator"),
        ("minecraft:powered_comparator", "minecraft:comparator"),
        ("minecraft:wall_banner", "minecraft:banner"),
        ("minecraft:standing_banner", "minecraft:banner"),
        ("minecraft:structure_block", "minecraft:structure_block"),
        ("minecraft:end_portal", "minecraft:end_portal"),
        ("minecraft:end_gateway", "minecraft:end_gateway"),
        ("minecraft:sign", "minecraft:sign"),
        ("minecraft:shield", "minecraft:banner"),
        ("minecraft:white_bed", "minecraft:bed"),
        ("minecraft:orange_bed", "minecraft:bed"),
        ("minecraft:magenta_bed", "minecraft:bed"),
        ("minecraft:light_blue_bed", "minecraft:bed"),
        ("minecraft:yellow_bed", "minecraft:bed"),
        ("minecraft:lime_bed", "minecraft:bed"),
        ("minecraft:pink_bed", "minecraft:bed"),
        ("minecraft:gray_bed", "minecraft:bed"),
        ("minecraft:silver_bed", "minecraft:bed"),
        ("minecraft:light_gray_bed", "minecraft:bed"),
        ("minecraft:cyan_bed", "minecraft:bed"),
        ("minecraft:purple_bed", "minecraft:bed"),
        ("minecraft:blue_bed", "minecraft:bed"),
        ("minecraft:brown_bed", "minecraft:bed"),
        ("minecraft:green_bed", "minecraft:bed"),
        ("minecraft:red_bed", "minecraft:bed"),
        ("minecraft:black_bed", "minecraft:bed"),
        ("minecraft:oak_sign", "minecraft:sign"),
        ("minecraft:spruce_sign", "minecraft:sign"),
        ("minecraft:birch_sign", "minecraft:sign"),
        ("minecraft:jungle_sign", "minecraft:sign"),
        ("minecraft:acacia_sign", "minecraft:sign"),
        ("minecraft:dark_oak_sign", "minecraft:sign"),
        ("minecraft:crimson_sign", "minecraft:sign"),
        ("minecraft:warped_sign", "minecraft:sign"),
        ("minecraft:skeleton_skull", "minecraft:skull"),
        ("minecraft:wither_skeleton_skull", "minecraft:skull"),
        ("minecraft:zombie_head", "minecraft:skull"),
        ("minecraft:player_head", "minecraft:skull"),
        ("minecraft:creeper_head", "minecraft:skull"),
        ("minecraft:dragon_head", "minecraft:skull"),
        ("minecraft:barrel", "minecraft:barrel"),
        ("minecraft:conduit", "minecraft:conduit"),
        ("minecraft:smoker", "minecraft:smoker"),
        ("minecraft:blast_furnace", "minecraft:blast_furnace"),
        ("minecraft:lectern", "minecraft:lectern"),
        ("minecraft:bell", "minecraft:bell"),
        ("minecraft:jigsaw", "minecraft:jigsaw"),
        ("minecraft:campfire", "minecraft:campfire"),
        ("minecraft:bee_nest", "minecraft:beehive"),
        ("minecraft:beehive", "minecraft:beehive"),
        ("minecraft:sculk_sensor", "minecraft:sculk_sensor"),
        ("minecraft:decorated_pot", "minecraft:decorated_pot"),
        ("minecraft:crafter", "minecraft:crafter"),
    ])
});

/// `V704.ADD_NAMES`.
pub(crate) static ADD_NAMES: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, value| {
        v99::add_names(
            &Dynamic::new(*ops, value),
            &ITEM_TO_BLOCKENTITY,
            &v99::ITEM_TO_ENTITY,
        )
    })
});

pub struct V704;

impl SchemaDef for V704 {
    fn namespaces_choice(&self, ty: &TypeReference) -> bool {
        *ty == References::BLOCK_ENTITY
    }

    fn register_block_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_block_entities(schema);
        for (old_id, new_id) in block_entity_id_fix::id_map() {
            let supplier = map
                .remove(old_id)
                .unwrap_or_else(|| panic!("Didn't find {old_id} in schema"));
            map.put_supplier(new_id, supplier);
        }
        map
    }

    fn register_types(
        &self,
        this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        this.super_register_types(schema, entity_types, block_entity_types);
        let block_entity_types = block_entity_types.clone();
        schema.register_type(true, &References::BLOCK_ENTITY, move |s| {
            DSL::optional_fields_with_rest(
                &[("components", References::DATA_COMPONENTS.in_(s))],
                &DSL::tagged_choice_lazy("id", &DSL::namespaced_string(), &block_entity_types, s),
            )
        });
        schema.register_type(true, &References::ITEM_STACK, |s| {
            DSL::hook(
                &DSL::optional_fields(&[
                    ("id", References::ITEM_NAME.in_(s)),
                    ("tag", v99::item_stack_tag(s)),
                ]),
                &ADD_NAMES,
                &HookFunction::identity(),
            )
        });
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V704)
}
