//! `EntityBlockStateFix`: replaces the numeric or named block (`Block`/`TileID`/`Tile`, `carried`, `inTile`,
//! `DisplayTile`) of falling blocks, endermen, arrows and minecarts by a block state, and drops `inTile` from the other
//! projectiles.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::block_state_data;
use crate::fixes::references::References;
use crate::fixes::tables::entity_block_state_fix::get_block_id;
use crate::schema::Schema;
use crate::ty::Type;
use crate::typed::Typed;
use crate::value::Value;
use mc_dfu::{Dyn, Dynamic, NBT};

/// `EntityBlockStateFix.getBlockId(name)`: the legacy numeric block id, 0 when unknown.
pub fn block_id(name: &str) -> i32 {
    get_block_id(name)
}

fn block_name_type() -> Type {
    DSL::named_type(
        References::BLOCK_NAME.type_name(),
        &DSL::or_type(&DSL::int_type(), &DSL::namespaced_string()),
    )
}

fn block_state(block: i32, data: i32) -> Value {
    Value::named(
        References::BLOCK_STATE.type_name(),
        Value::Dynamic(Dynamic::new(
            NBT,
            Dyn::from(block_state_data::get_tag(block << 4 | data)),
        )),
    )
}

/// `block.getSecond().map(i -> i, EntityBlockStateFix::getBlockId)` over a `BLOCK_NAME` pair.
fn block_of(named: Value) -> i32 {
    match named.into_pair().1 {
        Value::Left(id) => id.as_int(),
        Value::Right(name) => block_id(name.as_lossy_str()),
        other => panic!("expected an Either, got {other:?}"),
    }
}

fn update_falling_block(input: Typed) -> Typed {
    let old_type = DSL::optional_type(&DSL::field_type("Block", &block_name_type()));
    let new_type = DSL::optional_type(&DSL::field_type(
        "BlockState",
        &DSL::named_type(References::BLOCK_STATE.type_name(), &DSL::remainder_type()),
    ));
    let tag = input.get(&DSL::remainder_finder()).into_dynamic();
    let tile_tag = tag.clone();
    input
        .update_as(&old_type.finder(), &new_type, move |tile| {
            let block = match tile {
                Value::Left(l) => block_of(*l),
                Value::Right(_) => match tile_tag.get("TileID").as_number().result() {
                    Some(tile_id) => tile_id.int_value(),
                    None => i32::from(tile_tag.get("Tile").as_byte_or(0)) & 0xFF,
                },
                other => panic!("expected an Either, got {other:?}"),
            };
            let data = tile_tag.get("Data").as_int_or(0) & 15;
            Value::left(block_state(block, data))
        })
        .set_remainder(tag.remove("Data").remove("TileID").remove("Tile"))
}

fn update_block_to_block_state(
    input: Typed,
    old_field_name: &str,
    data_name: &str,
    new_field_name: &str,
) -> Typed {
    let old_type = DSL::field_type(old_field_name, &block_name_type());
    let new_type = DSL::field_type(
        new_field_name,
        &DSL::named_type(References::BLOCK_STATE.type_name(), &DSL::remainder_type()),
    );
    let tag = input.get_remainder();
    let data_tag = tag.clone();
    let data_name_owned = data_name.to_string();
    input
        .update_as(&old_type.finder(), &new_type, move |tile| {
            let block = block_of(tile);
            let data = data_tag.get(&data_name_owned).as_int_or(0) & 15;
            block_state(block, data)
        })
        .set_remainder(tag.remove(data_name))
}

fn update_entity(
    input: Typed,
    schemas: &(Schema, Schema),
    name: &str,
    function: impl FnMut(Typed) -> Typed,
) -> Typed {
    let old_type = schemas.0.get_choice_type(&References::ENTITY, name);
    let new_type = schemas.1.get_choice_type(&References::ENTITY, name);
    input.update_typed_as(&DSL::named_choice(name, &old_type), &new_type, function)
}

/// `new EntityBlockStateFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new("EntityBlockStateFix", output_schema, changes_type, |df| {
        let old_projectile_type = DSL::and_type(
            &DSL::optional_type(&DSL::field_type("inTile", &block_name_type())),
            &DSL::remainder_type(),
        );
        let old_projectile_finder = old_projectile_type.finder();
        let input_type = df.input_schema().get_type(&References::ENTITY);
        let output_type = df.output_schema().get_type(&References::ENTITY);
        let schemas = (df.input_schema(), df.output_schema());
        df.fix_type_everywhere_typed_new(
            "EntityBlockStateFix",
            &input_type,
            &output_type,
            move |input| {
                let df = &schemas;
                let minecart = |input: Typed| {
                    update_block_to_block_state(input, "DisplayTile", "DisplayData", "DisplayState")
                };
                let arrow = |input: Typed| {
                    update_block_to_block_state(input, "inTile", "inData", "inBlockState")
                };
                let remove_in_tile = |input: Typed| {
                    input.update_as(&old_projectile_finder, &DSL::remainder_type(), |pair| {
                        pair.into_pair().1
                    })
                };
                let mut input =
                    update_entity(input, df, "minecraft:falling_block", update_falling_block);
                input = update_entity(input, df, "minecraft:enderman", |input| {
                    update_block_to_block_state(
                        input,
                        "carried",
                        "carriedData",
                        "carriedBlockState",
                    )
                });
                input = update_entity(input, df, "minecraft:arrow", arrow);
                input = update_entity(input, df, "minecraft:spectral_arrow", arrow);
                for name in [
                    "minecraft:egg",
                    "minecraft:ender_pearl",
                    "minecraft:fireball",
                    "minecraft:potion",
                    "minecraft:small_fireball",
                    "minecraft:snowball",
                    "minecraft:wither_skull",
                    "minecraft:xp_bottle",
                ] {
                    input = update_entity(input, df, name, remove_in_tile);
                }
                for name in [
                    "minecraft:commandblock_minecart",
                    "minecraft:minecart",
                    "minecraft:chest_minecart",
                    "minecraft:furnace_minecart",
                    "minecraft:tnt_minecart",
                    "minecraft:hopper_minecart",
                    "minecraft:spawner_minecart",
                ] {
                    input = update_entity(input, df, name, minecart);
                }
                input
            },
        )
    })
}
