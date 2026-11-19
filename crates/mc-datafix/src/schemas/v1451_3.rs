//! `net.minecraft.util.datafix.schemas.V1451_3`.

use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::{Schema, SchemaDef, TemplateMap};

pub struct V1451_3;

impl SchemaDef for V1451_3 {
    fn register_entities(&self, this: &Schema, schema: &Schema) -> TemplateMap {
        let mut map = this.super_register_entities(schema);
        schema.register_simple(&mut map, "minecraft:egg");
        schema.register_simple(&mut map, "minecraft:ender_pearl");
        schema.register_simple(&mut map, "minecraft:fireball");
        schema.register_with(&mut map, "minecraft:potion", |s| {
            DSL::optional_fields(&[("Potion", References::ITEM_STACK.in_(s))])
        });
        schema.register_simple(&mut map, "minecraft:small_fireball");
        schema.register_simple(&mut map, "minecraft:snowball");
        schema.register_simple(&mut map, "minecraft:wither_skull");
        schema.register_simple(&mut map, "minecraft:xp_bottle");
        schema.register_with(&mut map, "minecraft:arrow", |s| {
            DSL::optional_fields(&[("inBlockState", References::BLOCK_STATE.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:enderman", |s| {
            DSL::optional_fields(&[("carriedBlockState", References::BLOCK_STATE.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:falling_block", |s| {
            DSL::optional_fields(&[
                ("BlockState", References::BLOCK_STATE.in_(s)),
                ("TileEntityData", References::BLOCK_ENTITY.in_(s)),
            ])
        });
        schema.register_with(&mut map, "minecraft:spectral_arrow", |s| {
            DSL::optional_fields(&[("inBlockState", References::BLOCK_STATE.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:chest_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayState", References::BLOCK_STATE.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        schema.register_with(&mut map, "minecraft:commandblock_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayState", References::BLOCK_STATE.in_(s)),
                ("LastOutput", References::TEXT_COMPONENT.in_(s)),
            ])
        });
        schema.register_with(&mut map, "minecraft:furnace_minecart", |s| {
            DSL::optional_fields(&[("DisplayState", References::BLOCK_STATE.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:hopper_minecart", |s| {
            DSL::optional_fields(&[
                ("DisplayState", References::BLOCK_STATE.in_(s)),
                ("Items", DSL::list(&References::ITEM_STACK.in_(s))),
            ])
        });
        schema.register_with(&mut map, "minecraft:minecart", |s| {
            DSL::optional_fields(&[("DisplayState", References::BLOCK_STATE.in_(s))])
        });
        schema.register_with(&mut map, "minecraft:spawner_minecart", |s| {
            DSL::optional_fields_with_rest(
                &[("DisplayState", References::BLOCK_STATE.in_(s))],
                &References::UNTAGGED_SPAWNER.in_(s),
            )
        });
        schema.register_with(&mut map, "minecraft:tnt_minecart", |s| {
            DSL::optional_fields(&[("DisplayState", References::BLOCK_STATE.in_(s))])
        });
        map
    }
    fn namespaced(&self) -> bool {
        true
    }
}

pub fn new(key: i32, parent: Option<Schema>) -> Schema {
    Schema::new(key, parent, V1451_3)
}
