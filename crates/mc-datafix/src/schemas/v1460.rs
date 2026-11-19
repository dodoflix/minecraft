//! `net.minecraft.util.datafix.schemas.V1460`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::schemas::v99;
use crate::schemas::{v705, v1451_6, v1458};
use crate::ty::HookFunction;

fn register_mob(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_simple(map, name);
}

fn register_inventory(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, v1458::nameable_inventory);
}

pub struct V1460;

impl SchemaDef for V1460 {
    fn register_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register_with(&mut map, "minecraft:area_effect_cloud", |s| {
            DSL::optional_fields(&[("Particle", References::PARTICLE.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:armor_stand");
        schema.register_with(&mut map, "minecraft:arrow", |s| {
            DSL::optional_fields(&[("inBlockState", References::BLOCK_STATE.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:bat");
        register_mob(schema, &mut map, "minecraft:blaze");
        schema.register_simple(&mut map, "minecraft:boat");
        register_mob(schema, &mut map, "minecraft:cave_spider");
        schema.register_with(&mut map, "minecraft:chest_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayState", References::BLOCK_STATE.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        register_mob(schema, &mut map, "minecraft:chicken");
        schema.register_with(&mut map, "minecraft:commandblock_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayState", References::BLOCK_STATE.in_(s)),
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
        schema.register_simple(&mut map, "minecraft:egg");
        register_mob(schema, &mut map, "minecraft:elder_guardian");
        schema.register_simple(&mut map, "minecraft:ender_crystal");
        register_mob(schema, &mut map, "minecraft:ender_dragon");
        schema.register_with(&mut map, "minecraft:enderman", |s| {
            DSL::optional_fields(&[("carriedBlockState", References::BLOCK_STATE.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:endermite");
        schema.register_simple(&mut map, "minecraft:ender_pearl");
        schema.register_simple(&mut map, "minecraft:evocation_fangs");
        register_mob(schema, &mut map, "minecraft:evocation_illager");
        schema.register_simple(&mut map, "minecraft:eye_of_ender_signal");
        schema.register_with(&mut map, "minecraft:falling_block", |s| {
            DSL::optional_fields(&[
                ("BlockState", References::BLOCK_STATE.in_(s)),
                ("TileEntityData", References::BLOCK_ENTITY.in_(s)),
            ])
        });
        schema.register_simple(&mut map, "minecraft:fireball");
        schema.register_with(&mut map, "minecraft:fireworks_rocket", |s| {
            DSL::optional_fields(&[("FireworksItem", References::ITEM_STACK.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:furnace_minecart", |s| {
            DSL::optional_fields(&[("DisplayState", References::BLOCK_STATE.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:ghast");
        register_mob(schema, &mut map, "minecraft:giant");
        register_mob(schema, &mut map, "minecraft:guardian");
        schema.register_with(&mut map, "minecraft:hopper_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayState", References::BLOCK_STATE.in_(s)),
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
        register_mob(schema, &mut map, "minecraft:illusion_illager");
        schema.register_with(&mut map, "minecraft:item", |s| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:item_frame", |s| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        schema.register_simple(&mut map, "minecraft:leash_knot");
        schema.register_with(&mut map, "minecraft:llama", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
                ("DecorItem", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_simple(&mut map, "minecraft:llama_spit");
        register_mob(schema, &mut map, "minecraft:magma_cube");
        schema.register_with(&mut map, "minecraft:minecart", |s| {
            DSL::optional_fields(&[("DisplayState", References::BLOCK_STATE.in_(s))])
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
            DSL::optional_fields(&[("Potion", References::ITEM_STACK.in_(s))])
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
        schema.register_simple(&mut map, "minecraft:small_fireball");
        schema.register_simple(&mut map, "minecraft:snowball");
        register_mob(schema, &mut map, "minecraft:snowman");
        schema.register_with(&mut map, "minecraft:spawner_minecart", |s| {
            DSL::optional_fields_with_rest(
                &[("DisplayState", References::BLOCK_STATE.in_(s))],
                &References::UNTAGGED_SPAWNER.in_(s),
            )
        });
        schema.register_with(&mut map, "minecraft:spectral_arrow", |s| {
            DSL::optional_fields(&[("inBlockState", References::BLOCK_STATE.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:spider");
        register_mob(schema, &mut map, "minecraft:squid");
        register_mob(schema, &mut map, "minecraft:stray");
        schema.register_simple(&mut map, "minecraft:tnt");
        schema.register_with(&mut map, "minecraft:tnt_minecart", |s| {
            DSL::optional_fields(&[("DisplayState", References::BLOCK_STATE.in_(s))])
        });
        register_mob(schema, &mut map, "minecraft:vex");
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
        register_mob(schema, &mut map, "minecraft:vindication_illager");
        register_mob(schema, &mut map, "minecraft:witch");
        register_mob(schema, &mut map, "minecraft:wither");
        register_mob(schema, &mut map, "minecraft:wither_skeleton");
        schema.register_simple(&mut map, "minecraft:wither_skull");
        register_mob(schema, &mut map, "minecraft:wolf");
        schema.register_simple(&mut map, "minecraft:xp_bottle");
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
        map
    }
    fn register_block_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        register_inventory(schema, &mut map, "minecraft:furnace");
        register_inventory(schema, &mut map, "minecraft:chest");
        register_inventory(schema, &mut map, "minecraft:trapped_chest");
        schema.register_simple(&mut map, "minecraft:ender_chest");
        schema.register_with(&mut map, "minecraft:jukebox", |s| {
            DSL::optional_fields(&[("RecordItem", References::ITEM_STACK.in_(s))])
        });
        register_inventory(schema, &mut map, "minecraft:dispenser");
        register_inventory(schema, &mut map, "minecraft:dropper");
        schema.register_with(&mut map, "minecraft:sign", v99::sign);
        schema.register_with(&mut map, "minecraft:mob_spawner", |s| {
            References::UNTAGGED_SPAWNER.in_(s)
        });
        schema.register_with(&mut map, "minecraft:piston", |s| {
            DSL::optional_fields(&[("blockState", References::BLOCK_STATE.in_(s))])
        });
        register_inventory(schema, &mut map, "minecraft:brewing_stand");
        schema.register_with(&mut map, "minecraft:enchanting_table", |s| {
            v1458::nameable(s)
        });
        schema.register_simple(&mut map, "minecraft:end_portal");
        schema.register_with(&mut map, "minecraft:beacon", v1458::nameable);
        schema.register_with(&mut map, "minecraft:skull", |s| {
            DSL::optional_fields(&[("custom_name", References::TEXT_COMPONENT.in_(s))])
        });
        schema.register_simple(&mut map, "minecraft:daylight_detector");
        register_inventory(schema, &mut map, "minecraft:hopper");
        schema.register_simple(&mut map, "minecraft:comparator");
        schema.register_with(&mut map, "minecraft:banner", v1458::nameable);
        schema.register_simple(&mut map, "minecraft:structure_block");
        schema.register_simple(&mut map, "minecraft:end_gateway");
        schema.register_with(&mut map, "minecraft:command_block", |s| {
            DSL::optional_fields(&[("LastOutput", References::TEXT_COMPONENT.in_(s))])
        });
        register_inventory(schema, &mut map, "minecraft:shulker_box");
        schema.register_simple(&mut map, "minecraft:bed");
        map
    }
    fn register_types(
        &self,
        _this: &Schema,
        schema: &Schema,
        entity_types: &TemplateMap,
        block_entity_types: &TemplateMap,
    ) {
        schema.register_type(false, &References::LEVEL, |s| {
            DSL::optional_fields_with_rest(
                &[(
                    "CustomBossEvents",
                    DSL::compound_list(&DSL::optional_fields(&[(
                        "Name",
                        References::TEXT_COMPONENT.in_(s),
                    )])),
                )],
                &References::LIGHTWEIGHT_LEVEL.in_(s),
            )
        });
        schema.register_type(false, &References::LIGHTWEIGHT_LEVEL, |_| DSL::remainder());
        schema.register_type(false, &References::RECIPE, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::PLAYER, |s| {
            DSL::optional_fields(&[
                (
                    "RootVehicle",
                    DSL::optional_fields(&[("Entity", References::ENTITY_TREE.in_(s))]),
                ),
                ("ender_pearls", DSL::list(&References::ENTITY_TREE.in_(s))),
                ("Inventory", DSL::list(&References::ITEM_STACK.in_(s))),
                ("EnderItems", DSL::list(&References::ITEM_STACK.in_(s))),
                ("ShoulderEntityLeft", References::ENTITY_TREE.in_(s)),
                ("ShoulderEntityRight", References::ENTITY_TREE.in_(s)),
                (
                    "recipeBook",
                    DSL::optional_fields(&[
                        ("recipes", DSL::list(&References::RECIPE.in_(s))),
                        ("toBeDisplayed", DSL::list(&References::RECIPE.in_(s))),
                    ]),
                ),
            ])
        });
        schema.register_type(false, &References::CHUNK, |s| {
            DSL::fields(&[(
                "Level",
                DSL::optional_fields(&[
                    ("Entities", DSL::list(&References::ENTITY_TREE.in_(s))),
                    (
                        "TileEntities",
                        DSL::list(&DSL::or(
                            &References::BLOCK_ENTITY.in_(s),
                            &DSL::remainder(),
                        )),
                    ),
                    (
                        "TileTicks",
                        DSL::list(&DSL::fields(&[("i", References::BLOCK_NAME.in_(s))])),
                    ),
                    (
                        "Sections",
                        DSL::list(&DSL::optional_fields(&[(
                            "Palette",
                            DSL::list(&References::BLOCK_STATE.in_(s)),
                        )])),
                    ),
                ]),
            )])
        });
        {
            let block_entity_types = block_entity_types.clone();
            schema.register_type(true, &References::BLOCK_ENTITY, move |s| {
                DSL::optional_fields_with_rest(
                    &[("components", References::DATA_COMPONENTS.in_(s))],
                    &DSL::tagged_choice_lazy(
                        "id",
                        &DSL::namespaced_string(),
                        &block_entity_types,
                        s,
                    ),
                )
            });
        }
        schema.register_type(true, &References::ENTITY_TREE, |s| {
            DSL::optional_fields_with_rest(
                &[("Passengers", DSL::list(&References::ENTITY_TREE.in_(s)))],
                &References::ENTITY.in_(s),
            )
        });
        {
            let entity_types = entity_types.clone();
            schema.register_type(true, &References::ENTITY, move |s| {
                DSL::and(
                    &References::ENTITY_EQUIPMENT.in_(s),
                    &DSL::optional_fields_with_rest(
                        &[("CustomName", References::TEXT_COMPONENT.in_(s))],
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
                &v705::ADD_NAMES,
                &HookFunction::identity(),
            )
        });
        schema.register_type(false, &References::HOTBAR, |s| {
            DSL::compound_list(&DSL::list(&References::ITEM_STACK.in_(s)))
        });
        schema.register_type(false, &References::OPTIONS, |_| DSL::remainder());
        schema.register_type(false, &References::STRUCTURE, |s| {
            DSL::optional_fields(&[
                (
                    "entities",
                    DSL::list(&DSL::optional_fields(&[(
                        "nbt",
                        References::ENTITY_TREE.in_(s),
                    )])),
                ),
                (
                    "blocks",
                    DSL::list(&DSL::optional_fields(&[(
                        "nbt",
                        References::BLOCK_ENTITY.in_(s),
                    )])),
                ),
                ("palette", DSL::list(&References::BLOCK_STATE.in_(s))),
                (
                    "palettes",
                    DSL::list(&DSL::list(&References::BLOCK_STATE.in_(s))),
                ),
            ])
        });
        schema.register_type(false, &References::BLOCK_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::ITEM_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::BLOCK_STATE, |_| DSL::remainder());
        schema.register_type(false, &References::FLAT_BLOCK_STATE, |_| DSL::remainder());
        schema.register_type(false, &References::STATS, |s| {
            let item_stats = || {
                DSL::compound_list_kv(
                    &References::ITEM_NAME.in_(s),
                    &DSL::const_type(&DSL::int_type()),
                )
            };
            let count = DSL::const_type(&DSL::int_type());
            DSL::optional_fields(&[(
                "stats",
                DSL::optional_fields(&[
                    (
                        "minecraft:mined",
                        DSL::compound_list_kv(&References::BLOCK_NAME.in_(s), &count),
                    ),
                    ("minecraft:crafted", item_stats()),
                    ("minecraft:used", item_stats()),
                    ("minecraft:broken", item_stats()),
                    ("minecraft:picked_up", item_stats()),
                    ("minecraft:dropped", item_stats()),
                    (
                        "minecraft:killed",
                        DSL::compound_list_kv(&References::ENTITY_NAME.in_(s), &count),
                    ),
                    (
                        "minecraft:killed_by",
                        DSL::compound_list_kv(&References::ENTITY_NAME.in_(s), &count),
                    ),
                    (
                        "minecraft:custom",
                        DSL::compound_list_kv(&DSL::const_type(&DSL::namespaced_string()), &count),
                    ),
                ]),
            )])
        });
        schema.register_type(false, &References::SAVED_DATA_COMMAND_STORAGE, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_CUSTOM_BOSS_EVENTS, |s| {
            DSL::optional_fields(&[(
                "data",
                DSL::compound_list(&DSL::optional_fields(&[(
                    "Name",
                    References::TEXT_COMPONENT.in_(s),
                )])),
            )])
        });
        schema.register_type(false, &References::SAVED_DATA_ENDER_DRAGON_FIGHT, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_GAME_RULES, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_TICKETS, |_| DSL::remainder());
        schema.register_type(false, &References::SAVED_DATA_MAP_DATA, |s| {
            DSL::optional_fields(&[(
                "data",
                DSL::optional_fields(&[(
                    "banners",
                    DSL::list(&DSL::optional_fields(&[(
                        "Name",
                        References::TEXT_COMPONENT.in_(s),
                    )])),
                )]),
            )])
        });
        schema.register_type(false, &References::SAVED_DATA_MAP_INDEX, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_RAIDS, |_| DSL::remainder());
        schema.register_type(false, &References::SAVED_DATA_RANDOM_SEQUENCES, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_SCHEDULED_EVENTS, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_SCOREBOARD, |s| {
            DSL::optional_fields(&[(
                "data",
                DSL::optional_fields(&[
                    ("Objectives", DSL::list(&References::OBJECTIVE.in_(s))),
                    ("Teams", DSL::list(&References::TEAM.in_(s))),
                    (
                        "PlayerScores",
                        DSL::list(&DSL::optional_fields(&[(
                            "display",
                            References::TEXT_COMPONENT.in_(s),
                        )])),
                    ),
                ]),
            )])
        });
        schema.register_type(false, &References::SAVED_DATA_STOPWATCHES, |_| {
            DSL::remainder()
        });
        schema.register_type(
            false,
            &References::SAVED_DATA_STRUCTURE_FEATURE_INDICES,
            |s| {
                DSL::optional_fields(&[(
                    "data",
                    DSL::optional_fields(&[(
                        "Features",
                        DSL::compound_list(&References::STRUCTURE_FEATURE.in_(s)),
                    )]),
                )])
            },
        );
        schema.register_type(false, &References::SAVED_DATA_WANDERING_TRADER, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_WEATHER, |_| DSL::remainder());
        schema.register_type(false, &References::SAVED_DATA_WORLD_BORDER, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_WORLD_CLOCKS, |_| {
            DSL::remainder()
        });
        schema.register_type(false, &References::SAVED_DATA_WORLD_GEN_SETTINGS, |s| {
            DSL::fields(&[("data", References::WORLD_GEN_SETTINGS.in_(s))])
        });
        schema.register_type(false, &References::DEBUG_PROFILE, |_| DSL::remainder());
        schema.register_type(false, &References::STRUCTURE_FEATURE, |_| DSL::remainder());
        let criterion_types = v1451_6::create_criterion_types();
        {
            let criterion_types = criterion_types.clone();
            schema.register_type(false, &References::OBJECTIVE, move |s| {
                DSL::hook(
                    &DSL::optional_fields(&[
                        (
                            "CriteriaType",
                            DSL::tagged_choice_lazy("type", &DSL::string(), &criterion_types, s),
                        ),
                        ("DisplayName", References::TEXT_COMPONENT.in_(s)),
                    ]),
                    &v1451_6::UNPACK_OBJECTIVE_ID,
                    &v1451_6::REPACK_OBJECTIVE_ID,
                )
            });
        }
        schema.register_type(false, &References::TEAM, |s| {
            DSL::optional_fields(&[
                ("MemberNamePrefix", References::TEXT_COMPONENT.in_(s)),
                ("MemberNameSuffix", References::TEXT_COMPONENT.in_(s)),
                ("DisplayName", References::TEXT_COMPONENT.in_(s)),
            ])
        });
        schema.register_type(true, &References::UNTAGGED_SPAWNER, |s| {
            DSL::optional_fields(&[
                (
                    "SpawnPotentials",
                    DSL::list(&DSL::fields(&[("Entity", References::ENTITY_TREE.in_(s))])),
                ),
                ("SpawnData", References::ENTITY_TREE.in_(s)),
            ])
        });
        schema.register_type(false, &References::ADVANCEMENTS, |s| {
            DSL::optional_fields(&[
                (
                    "minecraft:adventure/adventuring_time",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::BIOME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
                (
                    "minecraft:adventure/kill_a_mob",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
                (
                    "minecraft:adventure/kill_all_mobs",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
                (
                    "minecraft:husbandry/bred_all_animals",
                    DSL::optional_fields(&[(
                        "criteria",
                        DSL::compound_list_kv(
                            &References::ENTITY_NAME.in_(s),
                            &DSL::const_type(&DSL::string()),
                        ),
                    )]),
                ),
            ])
        });
        schema.register_type(false, &References::BIOME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::ENTITY_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::POI_CHUNK, |_| DSL::remainder());
        schema.register_type(false, &References::WORLD_GEN_SETTINGS, |_| DSL::remainder());
        schema.register_type(false, &References::ENTITY_CHUNK, |s| {
            DSL::optional_fields(&[("Entities", DSL::list(&References::ENTITY_TREE.in_(s)))])
        });
        schema.register_type(true, &References::DATA_COMPONENTS, |_| DSL::remainder());
        schema.register_type(true, &References::VILLAGER_TRADE, |s| {
            DSL::optional_fields(&[
                ("buy", References::ITEM_STACK.in_(s)),
                ("buyB", References::ITEM_STACK.in_(s)),
                ("sell", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_type(true, &References::PARTICLE, |_| {
            DSL::const_type(&DSL::string())
        });
        schema.register_type(true, &References::TEXT_COMPONENT, |_| {
            DSL::const_type(&DSL::string())
        });
        schema.register_type(true, &References::ENTITY_EQUIPMENT, |s| {
            DSL::and_list(&[
                DSL::optional(&DSL::field(
                    "ArmorItems",
                    &DSL::list(&References::ITEM_STACK.in_(s)),
                )),
                DSL::optional(&DSL::field(
                    "HandItems",
                    &DSL::list(&References::ITEM_STACK.in_(s)),
                )),
                DSL::optional(&DSL::field(
                    "body_armor_item",
                    &References::ITEM_STACK.in_(s),
                )),
                DSL::optional(&DSL::field("saddle", &References::ITEM_STACK.in_(s))),
            ])
        });
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1460)
}
