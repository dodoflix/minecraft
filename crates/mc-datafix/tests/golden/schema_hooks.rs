//! The hook functions the early schemas register (`ADD_NAMES` of the item stack types, the objective id hooks of
//! schema 1451.6), run through their types: read applies the pre hook, write the post hook.

use mc_datafix::{References, data_fixer};
use mc_dfu::{Dyn, Dynamic, NBT};

fn s(v: &str) -> Dyn {
    Dyn::Str(v.to_string())
}

/// Reads `input` as `ty` of schema `key` and writes it back.
fn round_trip(key: i32, ty: &mc_datafix::TypeReference, input: Dyn) -> Dyn {
    let ty = data_fixer().get_schema(key).get_type(ty);
    let (typed, _) = ty
        .read_typed(&Dynamic::new(NBT, input))
        .into_result()
        .expect("decode");
    typed.write().into_result().expect("encode").value
}

fn stack(id: &str, tag_field: &'static str) -> Dyn {
    Dyn::map_of([
        ("id", s(id)),
        ("tag", Dyn::map_of([(tag_field, Dyn::map_of([]))])),
    ])
}

fn assert_stack(out: &Dyn, tag_field: &str, expected_id: &str) {
    let id = out
        .map_get("tag")
        .and_then(|t| t.map_get(tag_field))
        .and_then(|t| t.map_get("id"));
    assert_eq!(id, Some(&s(expected_id)), "{out:?}");
}

#[test]
fn item_stacks_name_their_block_entity_and_entity() {
    let chest = round_trip(
        990,
        &References::ITEM_STACK,
        stack("minecraft:chest", "BlockEntityTag"),
    );
    assert_stack(&chest, "BlockEntityTag", "Chest");
    let stand = round_trip(
        990,
        &References::ITEM_STACK,
        stack("minecraft:armor_stand", "EntityTag"),
    );
    assert_stack(&stand, "EntityTag", "ArmorStand");
    let sign = round_trip(
        7040,
        &References::ITEM_STACK,
        stack("minecraft:oak_sign", "BlockEntityTag"),
    );
    assert_stack(&sign, "BlockEntityTag", "minecraft:sign");
    let pig = round_trip(
        7050,
        &References::ITEM_STACK,
        stack("minecraft:pig_spawn_egg", "EntityTag"),
    );
    assert_stack(&pig, "EntityTag", "minecraft:pig");
}

#[test]
fn objective_criteria_names_are_unpacked_on_read_and_repacked_on_write() {
    for name in [
        "minecraft.mined:minecraft.stone",
        "minecraft.custom:minecraft.jump",
        "dummy",
    ] {
        let input = Dyn::map_of([("CriteriaName", s(name)), ("DisplayName", s("\"x\""))]);
        let out = round_trip(14516, &References::OBJECTIVE, input.clone());
        assert!(out.java_eq(&input, false), "{name}: {out:?}");
    }
}

#[test]
fn schema_1460_reuses_the_item_stack_and_objective_hooks() {
    let chest = round_trip(
        14600,
        &References::ITEM_STACK,
        stack("minecraft:chest", "BlockEntityTag"),
    );
    assert_stack(&chest, "BlockEntityTag", "minecraft:chest");
    let input = Dyn::map_of([
        ("CriteriaName", s("minecraft.killed:minecraft.pig")),
        ("DisplayName", s("\"x\"")),
    ]);
    let out = round_trip(14600, &References::OBJECTIVE, input.clone());
    assert!(out.java_eq(&input, false), "{out:?}");
}
