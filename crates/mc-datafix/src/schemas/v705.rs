//! `net.minecraft.util.datafix.schemas.V705`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::ty::HookFunction;
use mc_dfu::Dynamic;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::schemas::{
    v99::{self, IdMap},
    v704,
};

/// `V705.ITEM_TO_ENTITY`.
static ITEM_TO_ENTITY: LazyLock<IdMap> = LazyLock::new(|| {
    HashMap::from([
        ("minecraft:armor_stand", "minecraft:armor_stand"),
        ("minecraft:painting", "minecraft:painting"),
        ("minecraft:armadillo_spawn_egg", "minecraft:armadillo"),
        ("minecraft:allay_spawn_egg", "minecraft:allay"),
        ("minecraft:axolotl_spawn_egg", "minecraft:axolotl"),
        ("minecraft:bat_spawn_egg", "minecraft:bat"),
        ("minecraft:bee_spawn_egg", "minecraft:bee"),
        ("minecraft:blaze_spawn_egg", "minecraft:blaze"),
        ("minecraft:bogged_spawn_egg", "minecraft:bogged"),
        ("minecraft:breeze_spawn_egg", "minecraft:breeze"),
        ("minecraft:cat_spawn_egg", "minecraft:cat"),
        ("minecraft:camel_spawn_egg", "minecraft:camel"),
        ("minecraft:cave_spider_spawn_egg", "minecraft:cave_spider"),
        ("minecraft:chicken_spawn_egg", "minecraft:chicken"),
        ("minecraft:cod_spawn_egg", "minecraft:cod"),
        ("minecraft:cow_spawn_egg", "minecraft:cow"),
        ("minecraft:creeper_spawn_egg", "minecraft:creeper"),
        ("minecraft:dolphin_spawn_egg", "minecraft:dolphin"),
        ("minecraft:donkey_spawn_egg", "minecraft:donkey"),
        ("minecraft:drowned_spawn_egg", "minecraft:drowned"),
        (
            "minecraft:elder_guardian_spawn_egg",
            "minecraft:elder_guardian",
        ),
        ("minecraft:ender_dragon_spawn_egg", "minecraft:ender_dragon"),
        ("minecraft:enderman_spawn_egg", "minecraft:enderman"),
        ("minecraft:endermite_spawn_egg", "minecraft:endermite"),
        ("minecraft:evoker_spawn_egg", "minecraft:evoker"),
        ("minecraft:fox_spawn_egg", "minecraft:fox"),
        ("minecraft:frog_spawn_egg", "minecraft:frog"),
        ("minecraft:ghast_spawn_egg", "minecraft:ghast"),
        ("minecraft:glow_squid_spawn_egg", "minecraft:glow_squid"),
        ("minecraft:goat_spawn_egg", "minecraft:goat"),
        ("minecraft:guardian_spawn_egg", "minecraft:guardian"),
        ("minecraft:hoglin_spawn_egg", "minecraft:hoglin"),
        ("minecraft:horse_spawn_egg", "minecraft:horse"),
        ("minecraft:husk_spawn_egg", "minecraft:husk"),
        ("minecraft:iron_golem_spawn_egg", "minecraft:iron_golem"),
        ("minecraft:llama_spawn_egg", "minecraft:llama"),
        ("minecraft:magma_cube_spawn_egg", "minecraft:magma_cube"),
        ("minecraft:mooshroom_spawn_egg", "minecraft:mooshroom"),
        ("minecraft:mule_spawn_egg", "minecraft:mule"),
        ("minecraft:ocelot_spawn_egg", "minecraft:ocelot"),
        ("minecraft:panda_spawn_egg", "minecraft:panda"),
        ("minecraft:parrot_spawn_egg", "minecraft:parrot"),
        ("minecraft:phantom_spawn_egg", "minecraft:phantom"),
        ("minecraft:pig_spawn_egg", "minecraft:pig"),
        ("minecraft:piglin_spawn_egg", "minecraft:piglin"),
        ("minecraft:piglin_brute_spawn_egg", "minecraft:piglin_brute"),
        ("minecraft:pillager_spawn_egg", "minecraft:pillager"),
        ("minecraft:polar_bear_spawn_egg", "minecraft:polar_bear"),
        ("minecraft:pufferfish_spawn_egg", "minecraft:pufferfish"),
        ("minecraft:rabbit_spawn_egg", "minecraft:rabbit"),
        ("minecraft:ravager_spawn_egg", "minecraft:ravager"),
        ("minecraft:salmon_spawn_egg", "minecraft:salmon"),
        ("minecraft:sheep_spawn_egg", "minecraft:sheep"),
        ("minecraft:shulker_spawn_egg", "minecraft:shulker"),
        ("minecraft:silverfish_spawn_egg", "minecraft:silverfish"),
        ("minecraft:skeleton_spawn_egg", "minecraft:skeleton"),
        (
            "minecraft:skeleton_horse_spawn_egg",
            "minecraft:skeleton_horse",
        ),
        ("minecraft:slime_spawn_egg", "minecraft:slime"),
        ("minecraft:sniffer_spawn_egg", "minecraft:sniffer"),
        ("minecraft:snow_golem_spawn_egg", "minecraft:snow_golem"),
        ("minecraft:spider_spawn_egg", "minecraft:spider"),
        ("minecraft:squid_spawn_egg", "minecraft:squid"),
        ("minecraft:stray_spawn_egg", "minecraft:stray"),
        ("minecraft:strider_spawn_egg", "minecraft:strider"),
        ("minecraft:tadpole_spawn_egg", "minecraft:tadpole"),
        ("minecraft:trader_llama_spawn_egg", "minecraft:trader_llama"),
        (
            "minecraft:tropical_fish_spawn_egg",
            "minecraft:tropical_fish",
        ),
        ("minecraft:turtle_spawn_egg", "minecraft:turtle"),
        ("minecraft:vex_spawn_egg", "minecraft:vex"),
        ("minecraft:villager_spawn_egg", "minecraft:villager"),
        ("minecraft:vindicator_spawn_egg", "minecraft:vindicator"),
        (
            "minecraft:wandering_trader_spawn_egg",
            "minecraft:wandering_trader",
        ),
        ("minecraft:warden_spawn_egg", "minecraft:warden"),
        ("minecraft:witch_spawn_egg", "minecraft:witch"),
        ("minecraft:wither_spawn_egg", "minecraft:wither"),
        (
            "minecraft:wither_skeleton_spawn_egg",
            "minecraft:wither_skeleton",
        ),
        ("minecraft:wolf_spawn_egg", "minecraft:wolf"),
        ("minecraft:zoglin_spawn_egg", "minecraft:zoglin"),
        ("minecraft:zombie_spawn_egg", "minecraft:zombie"),
        ("minecraft:zombie_horse_spawn_egg", "minecraft:zombie_horse"),
        (
            "minecraft:zombie_villager_spawn_egg",
            "minecraft:zombie_villager",
        ),
        (
            "minecraft:zombified_piglin_spawn_egg",
            "minecraft:zombified_piglin",
        ),
        ("minecraft:item_frame", "minecraft:item_frame"),
        ("minecraft:boat", "minecraft:oak_boat"),
        ("minecraft:oak_boat", "minecraft:oak_boat"),
        ("minecraft:oak_chest_boat", "minecraft:oak_chest_boat"),
        ("minecraft:spruce_boat", "minecraft:spruce_boat"),
        ("minecraft:spruce_chest_boat", "minecraft:spruce_chest_boat"),
        ("minecraft:birch_boat", "minecraft:birch_boat"),
        ("minecraft:birch_chest_boat", "minecraft:birch_chest_boat"),
        ("minecraft:jungle_boat", "minecraft:jungle_boat"),
        ("minecraft:jungle_chest_boat", "minecraft:jungle_chest_boat"),
        ("minecraft:acacia_boat", "minecraft:acacia_boat"),
        ("minecraft:acacia_chest_boat", "minecraft:acacia_chest_boat"),
        ("minecraft:cherry_boat", "minecraft:cherry_boat"),
        ("minecraft:cherry_chest_boat", "minecraft:cherry_chest_boat"),
        ("minecraft:dark_oak_boat", "minecraft:dark_oak_boat"),
        (
            "minecraft:dark_oak_chest_boat",
            "minecraft:dark_oak_chest_boat",
        ),
        ("minecraft:mangrove_boat", "minecraft:mangrove_boat"),
        (
            "minecraft:mangrove_chest_boat",
            "minecraft:mangrove_chest_boat",
        ),
        ("minecraft:bamboo_raft", "minecraft:bamboo_raft"),
        ("minecraft:bamboo_chest_raft", "minecraft:bamboo_chest_raft"),
        ("minecraft:minecart", "minecraft:minecart"),
        ("minecraft:chest_minecart", "minecraft:chest_minecart"),
        ("minecraft:furnace_minecart", "minecraft:furnace_minecart"),
        ("minecraft:tnt_minecart", "minecraft:tnt_minecart"),
        ("minecraft:hopper_minecart", "minecraft:hopper_minecart"),
    ])
});

/// `V705.ADD_NAMES`.
pub(crate) static ADD_NAMES: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, value| {
        v99::add_names(
            &Dynamic::new(*ops, value),
            &v704::ITEM_TO_BLOCKENTITY,
            &ITEM_TO_ENTITY,
        )
    })
});

fn register_mob(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_simple(map, name);
}

fn register_throwable_projectile(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, |s| {
        DSL::optional_fields(&[("inTile", References::BLOCK_NAME.in_(s))])
    });
}

pub struct V705;

impl SchemaDef for V705 {
    fn register_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register_with(&mut map, "minecraft:area_effect_cloud", |s| {
            DSL::optional_fields(&[("Particle", References::PARTICLE.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:armor_stand");
        schema.register_with(&mut map, "minecraft:arrow", |s| {
            DSL::optional_fields(&[("inTile", References::BLOCK_NAME.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:bat");
        register_mob(schema, &mut map, "minecraft:blaze");
        schema.register_simple(&mut map, "minecraft:boat");
        register_mob(schema, &mut map, "minecraft:cave_spider");
        schema.register_with(&mut map, "minecraft:chest_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayTile", References::BLOCK_NAME.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        register_mob(schema, &mut map, "minecraft:chicken");
        schema.register_with(&mut map, "minecraft:commandblock_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayTile", References::BLOCK_NAME.in_(s)),
                ("LastOutput", References::TEXT_COMPONENT.in_(s)),
            ])
        });
        register_mob(schema, &mut map, "minecraft:cow");
        register_mob(schema, &mut map, "minecraft:creeper");
        schema.register_with(&mut map, "minecraft:donkey", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_simple(&mut map, "minecraft:dragon_fireball");
        register_throwable_projectile(schema, &mut map, "minecraft:egg");
        register_mob(schema, &mut map, "minecraft:elder_guardian");
        schema.register_simple(&mut map, "minecraft:ender_crystal");
        register_mob(schema, &mut map, "minecraft:ender_dragon");
        schema.register_with(&mut map, "minecraft:enderman", |s| {
            DSL::optional_fields(&[("carried", References::BLOCK_NAME.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:endermite");
        register_throwable_projectile(schema, &mut map, "minecraft:ender_pearl");
        schema.register_simple(&mut map, "minecraft:eye_of_ender_signal");
        schema.register_with(&mut map, "minecraft:falling_block", |s| {
            DSL::optional_fields(&[
                ("Block", References::BLOCK_NAME.in_(s)),
                ("TileEntityData", References::BLOCK_ENTITY.in_(s)),
            ])
        });
        register_throwable_projectile(schema, &mut map, "minecraft:fireball");
        schema.register_with(&mut map, "minecraft:fireworks_rocket", |s| {
            DSL::optional_fields(&[("FireworksItem", References::ITEM_STACK.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:furnace_minecart", |s| {
            DSL::optional_fields(&[("DisplayTile", References::BLOCK_NAME.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:ghast");
        register_mob(schema, &mut map, "minecraft:giant");
        register_mob(schema, &mut map, "minecraft:guardian");
        schema.register_with(&mut map, "minecraft:hopper_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayTile", References::BLOCK_NAME.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        schema.register_with(&mut map, "minecraft:horse", |s| {
            DSL::optional_fields(&[
                ("ArmorItem", References::ITEM_STACK.in_(s)),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
            ])
        });
        register_mob(schema, &mut map, "minecraft:husk");
        schema.register_with(&mut map, "minecraft:item", |s| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:item_frame", |s| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        schema.register_simple(&mut map, "minecraft:leash_knot");
        register_mob(schema, &mut map, "minecraft:magma_cube");
        schema.register_with(&mut map, "minecraft:minecart", |s| {
            DSL::optional_fields(&[("DisplayTile", References::BLOCK_NAME.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:mooshroom");
        schema.register_with(&mut map, "minecraft:mule", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
            ])
        });
        register_mob(schema, &mut map, "minecraft:ocelot");
        schema.register_simple(&mut map, "minecraft:painting");
        register_mob(schema, &mut map, "minecraft:parrot");
        register_mob(schema, &mut map, "minecraft:pig");
        register_mob(schema, &mut map, "minecraft:polar_bear");
        schema.register_with(&mut map, "minecraft:potion", |s| {
            DSL::optional_fields(&[
                ("Potion", References::ITEM_STACK.in_(s)),
                ("inTile", References::BLOCK_NAME.in_(s)),
            ])
        });
        register_mob(schema, &mut map, "minecraft:rabbit");
        register_mob(schema, &mut map, "minecraft:sheep");
        register_mob(schema, &mut map, "minecraft:shulker");
        schema.register_simple(&mut map, "minecraft:shulker_bullet");
        register_mob(schema, &mut map, "minecraft:silverfish");
        register_mob(schema, &mut map, "minecraft:skeleton");
        schema.register_with(&mut map, "minecraft:skeleton_horse", |s| {
            DSL::optional_fields(&[("SaddleItem", References::ITEM_STACK.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:slime");
        register_throwable_projectile(schema, &mut map, "minecraft:small_fireball");
        register_throwable_projectile(schema, &mut map, "minecraft:snowball");
        register_mob(schema, &mut map, "minecraft:snowman");
        schema.register_with(&mut map, "minecraft:spawner_minecart", |s| {
            DSL::optional_fields_with_rest(
                &[("DisplayTile", References::BLOCK_NAME.in_(s))],
                &References::UNTAGGED_SPAWNER.in_(s),
            )
        });
        schema.register_with(&mut map, "minecraft:spectral_arrow", |s| {
            DSL::optional_fields(&[("inTile", References::BLOCK_NAME.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:spider");
        register_mob(schema, &mut map, "minecraft:squid");
        register_mob(schema, &mut map, "minecraft:stray");
        schema.register_simple(&mut map, "minecraft:tnt");
        schema.register_with(&mut map, "minecraft:tnt_minecart", |s| {
            DSL::optional_fields(&[("DisplayTile", References::BLOCK_NAME.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:villager", |s| {
            DSL::optional_fields(&[
                ("Inventory", DSL::list(&References::ITEM_STACK.in_(s))),
                (
                    "Offers",
                    DSL::optional_fields(&[(
                        "Recipes",
                        DSL::list(&References::VILLAGER_TRADE.in_(s)),
                    )]),
                ),
            ])
        });
        register_mob(schema, &mut map, "minecraft:villager_golem");
        register_mob(schema, &mut map, "minecraft:witch");
        register_mob(schema, &mut map, "minecraft:wither");
        register_mob(schema, &mut map, "minecraft:wither_skeleton");
        register_throwable_projectile(schema, &mut map, "minecraft:wither_skull");
        register_mob(schema, &mut map, "minecraft:wolf");
        register_throwable_projectile(schema, &mut map, "minecraft:xp_bottle");
        schema.register_simple(&mut map, "minecraft:xp_orb");
        register_mob(schema, &mut map, "minecraft:zombie");
        schema.register_with(&mut map, "minecraft:zombie_horse", |s| {
            DSL::optional_fields(&[("SaddleItem", References::ITEM_STACK.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:zombie_pigman");
        schema.register_with(&mut map, "minecraft:zombie_villager", |s| {
            DSL::optional_fields(&[(
                "Offers",
                DSL::optional_fields(&[("Recipes", DSL::list(&References::VILLAGER_TRADE.in_(s)))]),
            )])
        });
        schema.register_simple(&mut map, "minecraft:evocation_fangs");
        register_mob(schema, &mut map, "minecraft:evocation_illager");
        register_mob(schema, &mut map, "minecraft:illusion_illager");
        schema.register_with(&mut map, "minecraft:llama", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
                ("DecorItem", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_simple(&mut map, "minecraft:llama_spit");
        register_mob(schema, &mut map, "minecraft:vex");
        register_mob(schema, &mut map, "minecraft:vindication_illager");
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
        {
            let entity_types = entity_types.clone();
            schema.register_type(true, &References::ENTITY, move |s| {
                DSL::and(
                    &References::ENTITY_EQUIPMENT.in_(s),
                    &DSL::optional_fields_with_rest(
                        &[("CustomName", DSL::const_type(&DSL::string()))],
                        &DSL::tagged_choice_lazy("id", &DSL::namespaced_string(), &entity_types, s),
                    ),
                )
            });
        }
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
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V705)
}
