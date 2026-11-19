//! `net.minecraft.util.datafix.schemas.V99`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};
use crate::template::Template;
use crate::ty::HookFunction;
use crate::ty::ensure_namespaced;
use mc_dfu::{Dyn, Dynamic};
use std::collections::HashMap;
use std::sync::LazyLock;

pub(crate) type IdMap = HashMap<&'static str, &'static str>;

/// `V99.ITEM_TO_BLOCKENTITY` (only read through [`ADD_NAMES`]).
static ITEM_TO_BLOCKENTITY: LazyLock<IdMap> = LazyLock::new(|| {
    HashMap::from([
        ("minecraft:furnace", "Furnace"),
        ("minecraft:lit_furnace", "Furnace"),
        ("minecraft:chest", "Chest"),
        ("minecraft:trapped_chest", "Chest"),
        ("minecraft:ender_chest", "EnderChest"),
        ("minecraft:jukebox", "RecordPlayer"),
        ("minecraft:dispenser", "Trap"),
        ("minecraft:dropper", "Dropper"),
        ("minecraft:sign", "Sign"),
        ("minecraft:mob_spawner", "MobSpawner"),
        ("minecraft:noteblock", "Music"),
        ("minecraft:brewing_stand", "Cauldron"),
        ("minecraft:enhanting_table", "EnchantTable"),
        ("minecraft:command_block", "CommandBlock"),
        ("minecraft:beacon", "Beacon"),
        ("minecraft:skull", "Skull"),
        ("minecraft:daylight_detector", "DLDetector"),
        ("minecraft:hopper", "Hopper"),
        ("minecraft:banner", "Banner"),
        ("minecraft:flower_pot", "FlowerPot"),
        ("minecraft:repeating_command_block", "CommandBlock"),
        ("minecraft:chain_command_block", "CommandBlock"),
        ("minecraft:standing_sign", "Sign"),
        ("minecraft:wall_sign", "Sign"),
        ("minecraft:piston_head", "Piston"),
        ("minecraft:daylight_detector_inverted", "DLDetector"),
        ("minecraft:unpowered_comparator", "Comparator"),
        ("minecraft:powered_comparator", "Comparator"),
        ("minecraft:wall_banner", "Banner"),
        ("minecraft:standing_banner", "Banner"),
        ("minecraft:structure_block", "Structure"),
        ("minecraft:end_portal", "Airportal"),
        ("minecraft:end_gateway", "EndGateway"),
        ("minecraft:shield", "Banner"),
    ])
});

/// `V99.ITEM_TO_ENTITY`.
pub(crate) static ITEM_TO_ENTITY: LazyLock<IdMap> = LazyLock::new(|| {
    HashMap::from([
        ("minecraft:armor_stand", "ArmorStand"),
        ("minecraft:painting", "Painting"),
    ])
});

/// `V99.ADD_NAMES`.
pub(crate) static ADD_NAMES: LazyLock<HookFunction> = LazyLock::new(|| {
    HookFunction::new(|ops, value| {
        add_names(
            &Dynamic::new(*ops, value),
            &ITEM_TO_BLOCKENTITY,
            &ITEM_TO_ENTITY,
        )
    })
});

fn register_throwable_projectile(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, |s| {
        DSL::optional_fields(&[("inTile", References::BLOCK_NAME.in_(s))])
    });
}

fn register_minecart(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, |s| {
        DSL::optional_fields(&[("DisplayTile", References::BLOCK_NAME.in_(s))])
    });
}

fn register_inventory(schema: &Schema, map: &mut TemplateMap, name: &str) {
    schema.register_with(map, name, |s| {
        DSL::optional_fields(&[("Items", DSL::list(&References::ITEM_STACK.in_(s)))])
    });
}

pub struct V99;

impl SchemaDef for V99 {
    fn register_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        schema.register_with(&mut map, "Item", |s| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        schema.register_simple(&mut map, "XPOrb");
        register_throwable_projectile(schema, &mut map, "ThrownEgg");
        schema.register_simple(&mut map, "LeashKnot");
        schema.register_simple(&mut map, "Painting");
        schema.register_with(&mut map, "Arrow", |s| {
            DSL::optional_fields(&[("inTile", References::BLOCK_NAME.in_(s))])
        });
        schema.register_with(&mut map, "TippedArrow", |s| {
            DSL::optional_fields(&[("inTile", References::BLOCK_NAME.in_(s))])
        });
        schema.register_with(&mut map, "SpectralArrow", |s| {
            DSL::optional_fields(&[("inTile", References::BLOCK_NAME.in_(s))])
        });
        register_throwable_projectile(schema, &mut map, "Snowball");
        register_throwable_projectile(schema, &mut map, "Fireball");
        register_throwable_projectile(schema, &mut map, "SmallFireball");
        register_throwable_projectile(schema, &mut map, "ThrownEnderpearl");
        schema.register_simple(&mut map, "EyeOfEnderSignal");
        schema.register_with(&mut map, "ThrownPotion", |s| {
            DSL::optional_fields(&[
                ("inTile", References::BLOCK_NAME.in_(s)),
                ("Potion", References::ITEM_STACK.in_(s)),
            ])
        });
        register_throwable_projectile(schema, &mut map, "ThrownExpBottle");
        schema.register_with(&mut map, "ItemFrame", |s| {
            DSL::optional_fields(&[("Item", References::ITEM_STACK.in_(s))])
        });
        register_throwable_projectile(schema, &mut map, "WitherSkull");
        schema.register_simple(&mut map, "PrimedTnt");
        schema.register_with(&mut map, "FallingSand", |s| {
            DSL::optional_fields(&[
                ("Block", References::BLOCK_NAME.in_(s)),
                ("TileEntityData", References::BLOCK_ENTITY.in_(s)),
            ])
        });
        schema.register_with(&mut map, "FireworksRocketEntity", |s| {
            DSL::optional_fields(&[("FireworksItem", References::ITEM_STACK.in_(s))])
        });
        schema.register_simple(&mut map, "Boat");
        schema.register_with(&mut map, "Minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayTile", References::BLOCK_NAME.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        register_minecart(schema, &mut map, "MinecartRideable");
        schema.register_with(&mut map, "MinecartChest", |s| {
            DSL::optional_fields(&[
                ("DisplayTile", References::BLOCK_NAME.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        register_minecart(schema, &mut map, "MinecartFurnace");
        register_minecart(schema, &mut map, "MinecartTNT");
        schema.register_with(&mut map, "MinecartSpawner", |s| {
            DSL::optional_fields_with_rest(
                &[("DisplayTile", References::BLOCK_NAME.in_(s))],
                &References::UNTAGGED_SPAWNER.in_(s),
            )
        });
        schema.register_with(&mut map, "MinecartHopper", |s| {
            DSL::optional_fields(&[
                ("DisplayTile", References::BLOCK_NAME.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        schema.register_with(&mut map, "MinecartCommandBlock", |s| {
            DSL::optional_fields(&[
                ("DisplayTile", References::BLOCK_NAME.in_(s)),
                ("LastOutput", References::TEXT_COMPONENT.in_(s)),
            ])
        });
        schema.register_simple(&mut map, "ArmorStand");
        schema.register_simple(&mut map, "Creeper");
        schema.register_simple(&mut map, "Skeleton");
        schema.register_simple(&mut map, "Spider");
        schema.register_simple(&mut map, "Giant");
        schema.register_simple(&mut map, "Zombie");
        schema.register_simple(&mut map, "Slime");
        schema.register_simple(&mut map, "Ghast");
        schema.register_simple(&mut map, "PigZombie");
        schema.register_with(&mut map, "Enderman", |s| {
            DSL::optional_fields(&[("carried", References::BLOCK_NAME.in_(s))])
        });
        schema.register_simple(&mut map, "CaveSpider");
        schema.register_simple(&mut map, "Silverfish");
        schema.register_simple(&mut map, "Blaze");
        schema.register_simple(&mut map, "LavaSlime");
        schema.register_simple(&mut map, "EnderDragon");
        schema.register_simple(&mut map, "WitherBoss");
        schema.register_simple(&mut map, "Bat");
        schema.register_simple(&mut map, "Witch");
        schema.register_simple(&mut map, "Endermite");
        schema.register_simple(&mut map, "Guardian");
        schema.register_simple(&mut map, "Pig");
        schema.register_simple(&mut map, "Sheep");
        schema.register_simple(&mut map, "Cow");
        schema.register_simple(&mut map, "Chicken");
        schema.register_simple(&mut map, "Squid");
        schema.register_simple(&mut map, "Wolf");
        schema.register_simple(&mut map, "MushroomCow");
        schema.register_simple(&mut map, "SnowMan");
        schema.register_simple(&mut map, "Ozelot");
        schema.register_simple(&mut map, "VillagerGolem");
        schema.register_with(&mut map, "EntityHorse", |s| {
            DSL::optional_fields(&[
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
                ("ArmorItem", References::ITEM_STACK.in_(s)),
                ("SaddleItem", References::ITEM_STACK.in_(s)),
            ])
        });
        schema.register_simple(&mut map, "Rabbit");
        schema.register_with(&mut map, "Villager", |s| {
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
        schema.register_simple(&mut map, "EnderCrystal");
        schema.register_with(&mut map, "AreaEffectCloud", |s| {
            DSL::optional_fields(&[("Particle", References::PARTICLE.in_(s))])
        });
        schema.register_simple(&mut map, "ShulkerBullet");
        schema.register_simple(&mut map, "DragonFireball");
        schema.register_simple(&mut map, "Shulker");
        map
    }
    fn register_block_entities(&self, _this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = TemplateMap::new();
        register_inventory(schema, &mut map, "Furnace");
        register_inventory(schema, &mut map, "Chest");
        schema.register_simple(&mut map, "EnderChest");
        schema.register_with(&mut map, "RecordPlayer", |s| {
            DSL::optional_fields(&[("RecordItem", References::ITEM_STACK.in_(s))])
        });
        register_inventory(schema, &mut map, "Trap");
        register_inventory(schema, &mut map, "Dropper");
        schema.register_with(&mut map, "Sign", sign);
        schema.register_with(&mut map, "MobSpawner", |s| {
            References::UNTAGGED_SPAWNER.in_(s)
        });
        schema.register_simple(&mut map, "Music");
        schema.register_simple(&mut map, "Piston");
        register_inventory(schema, &mut map, "Cauldron");
        schema.register_simple(&mut map, "EnchantTable");
        schema.register_simple(&mut map, "Airportal");
        schema.register_with(&mut map, "Control", |s| {
            DSL::optional_fields(&[("LastOutput", References::TEXT_COMPONENT.in_(s))])
        });
        schema.register_simple(&mut map, "Beacon");
        schema.register_with(&mut map, "Skull", |s| {
            DSL::optional_fields(&[("custom_name", References::TEXT_COMPONENT.in_(s))])
        });
        schema.register_simple(&mut map, "DLDetector");
        register_inventory(schema, &mut map, "Hopper");
        schema.register_simple(&mut map, "Comparator");
        schema.register_with(&mut map, "FlowerPot", |s| {
            DSL::optional_fields(&[(
                "Item",
                DSL::or(
                    &DSL::const_type(&DSL::int_type()),
                    &References::ITEM_NAME.in_(s),
                ),
            )])
        });
        schema.register_with(&mut map, "Banner", |s| {
            DSL::optional_fields(&[("CustomName", References::TEXT_COMPONENT.in_(s))])
        });
        schema.register_simple(&mut map, "Structure");
        schema.register_simple(&mut map, "EndGateway");
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
        schema.register_type(false, &References::PLAYER, |s| {
            DSL::optional_fields(&[
                ("Inventory", DSL::list(&References::ITEM_STACK.in_(s))),
                ("EnderItems", DSL::list(&References::ITEM_STACK.in_(s))),
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
                ]),
            )])
        });
        {
            let block_entity_types = block_entity_types.clone();
            schema.register_type(true, &References::BLOCK_ENTITY, move |s| {
                DSL::optional_fields_with_rest(
                    &[("components", References::DATA_COMPONENTS.in_(s))],
                    &DSL::tagged_choice_lazy("id", &DSL::string(), &block_entity_types, s),
                )
            });
        }
        schema.register_type(true, &References::ENTITY_TREE, |s| {
            DSL::optional_fields_with_rest(
                &[("Riding", References::ENTITY_TREE.in_(s))],
                &References::ENTITY.in_(s),
            )
        });
        schema.register_type(false, &References::ENTITY_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        {
            let entity_types = entity_types.clone();
            schema.register_type(true, &References::ENTITY, move |s| {
                DSL::and(
                    &References::ENTITY_EQUIPMENT.in_(s),
                    &DSL::optional_fields_with_rest(
                        &[("CustomName", DSL::const_type(&DSL::string()))],
                        &DSL::tagged_choice_lazy("id", &DSL::string(), &entity_types, s),
                    ),
                )
            });
        }
        schema.register_type(true, &References::ITEM_STACK, |s| {
            DSL::hook(
                &DSL::optional_fields(&[
                    (
                        "id",
                        DSL::or(
                            &DSL::const_type(&DSL::int_type()),
                            &References::ITEM_NAME.in_(s),
                        ),
                    ),
                    ("tag", item_stack_tag(s)),
                ]),
                &ADD_NAMES,
                &HookFunction::identity(),
            )
        });
        schema.register_type(false, &References::OPTIONS, |_| DSL::remainder());
        schema.register_type(false, &References::BLOCK_NAME, |_| {
            DSL::or(
                &DSL::const_type(&DSL::int_type()),
                &DSL::const_type(&DSL::namespaced_string()),
            )
        });
        schema.register_type(false, &References::ITEM_NAME, |_| {
            DSL::const_type(&DSL::namespaced_string())
        });
        schema.register_type(false, &References::STATS, |_| DSL::remainder());
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
        schema.register_type(false, &References::OBJECTIVE, |_| DSL::remainder());
        schema.register_type(false, &References::TEAM, |s| {
            DSL::optional_fields(&[
                ("MemberNamePrefix", References::TEXT_COMPONENT.in_(s)),
                ("MemberNameSuffix", References::TEXT_COMPONENT.in_(s)),
                ("DisplayName", References::TEXT_COMPONENT.in_(s)),
            ])
        });
        schema.register_type(true, &References::UNTAGGED_SPAWNER, |_| DSL::remainder());
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
        schema.register_type(false, &References::BLOCK_STATE, |_| DSL::remainder());
        schema.register_type(false, &References::FLAT_BLOCK_STATE, |_| DSL::remainder());
        schema.register_type(true, &References::ENTITY_EQUIPMENT, |s| {
            DSL::optional(&DSL::field(
                "Equipment",
                &DSL::list(&References::ITEM_STACK.in_(s)),
            ))
        });
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V99)
}

/// `V99.sign`.
pub(crate) fn sign(schema: &Schema) -> Template {
    DSL::optional_fields(&[
        ("Text1", References::TEXT_COMPONENT.in_(schema)),
        ("Text2", References::TEXT_COMPONENT.in_(schema)),
        ("Text3", References::TEXT_COMPONENT.in_(schema)),
        ("Text4", References::TEXT_COMPONENT.in_(schema)),
        ("FilteredText1", References::TEXT_COMPONENT.in_(schema)),
        ("FilteredText2", References::TEXT_COMPONENT.in_(schema)),
        ("FilteredText3", References::TEXT_COMPONENT.in_(schema)),
        ("FilteredText4", References::TEXT_COMPONENT.in_(schema)),
    ])
}

/// `V99.itemStackTag`.
pub(crate) fn item_stack_tag(schema: &Schema) -> Template {
    DSL::optional_fields(&[
        ("EntityTag", References::ENTITY_TREE.in_(schema)),
        ("BlockEntityTag", References::BLOCK_ENTITY.in_(schema)),
        ("CanDestroy", DSL::list(&References::BLOCK_NAME.in_(schema))),
        ("CanPlaceOn", DSL::list(&References::BLOCK_NAME.in_(schema))),
        ("Items", DSL::list(&References::ITEM_STACK.in_(schema))),
        (
            "ChargedProjectiles",
            DSL::list(&References::ITEM_STACK.in_(schema)),
        ),
        ("pages", DSL::list(&References::TEXT_COMPONENT.in_(schema))),
        (
            "filtered_pages",
            DSL::compound_list(&References::TEXT_COMPONENT.in_(schema)),
        ),
        (
            "display",
            DSL::optional_fields(&[
                ("Name", References::TEXT_COMPONENT.in_(schema)),
                ("Lore", DSL::list(&References::TEXT_COMPONENT.in_(schema))),
            ]),
        ),
    ])
}

/// `V99.addNames`: names the block entity / entity of an item stack's `tag` after the item id.
pub(crate) fn add_names(
    input: &Dynamic,
    item_to_block_entity: &IdMap,
    item_to_entity: &IdMap,
) -> Dyn {
    input
        .update("tag", |item_stack_tag| {
            item_stack_tag
                .update("BlockEntityTag", |block_entity| {
                    let item_id = input
                        .get("id")
                        .as_string()
                        .result()
                        .map(|id| ensure_namespaced(&id))
                        .unwrap_or_else(|| "minecraft:air".to_string());
                    if item_id != "minecraft:air" {
                        if let Some(expected_id) = item_to_block_entity.get(item_id.as_str()) {
                            return block_entity.set("id", input.create_string(*expected_id));
                        }
                        log::warn!("Unable to resolve BlockEntity for ItemStack: {item_id}");
                    }
                    block_entity
                })
                .update("EntityTag", |entity| {
                    if entity.get("id").is_present() {
                        return entity;
                    }
                    let item_id = ensure_namespaced(&input.get("id").as_string_or(""));
                    match item_to_entity.get(item_id.as_str()) {
                        Some(expected_id) => entity.set("id", input.create_string(*expected_id)),
                        None => entity,
                    }
                })
        })
        .into_value()
}
