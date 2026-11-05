//! Codec accept/reject behaviour of the rule / processor types and the vanilla `worldgen/processor_list` registry.

use super::rule::blockentity::rule_block_entity_modifier;
use super::structure_processor::{decode_direct_list, decode_single};
use super::structure_processor_list::load_processor_lists;
use super::{pos_rule_test, rule_test};
use crate::world::level::levelgen::registries::{Cells, DecodeCtx};
use mc_core::pos::BlockPos;
use mc_core::random::{self, RandomSource};
use mc_data::block::Block;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{FsDataSource, WorldgenDataSource, WorldgenRegistries};
use serde_json::{Value, json};
use std::sync::Arc;

fn with_ctx<R>(f: impl FnOnce(&DecodeCtx<'_>) -> R) -> R {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/src/data");
    let source: Arc<dyn WorldgenDataSource> = Arc::new(FsDataSource::new(root));
    let worldgen = WorldgenRegistries::new(source.clone());
    let biomes = BiomeRegistry::load(&worldgen).expect("vanilla biomes");
    let cells = Cells::new(source);
    f(&DecodeCtx { worldgen: &worldgen, biomes: &biomes, cells: &cells })
}

fn state(name: &str) -> mc_data::block::BlockState {
    Block::by_name(name).unwrap().default_state()
}

fn rule_err(v: Value) -> String {
    with_ctx(|ctx| rule_test::decode(&v, ctx).err().expect("must be rejected").to_string())
}

fn processor_err(v: Value) -> String {
    with_ctx(|ctx| decode_single(&v, ctx).err().expect("must be rejected").to_string())
}

#[test]
fn vanilla_processor_lists_decode() {
    with_ctx(|ctx| {
        let registry = load_processor_lists(ctx).expect("vanilla processor lists decode");
        assert_eq!(registry.len(), 40);
        for (id, holder) in registry.iter() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../ref/src/data/minecraft/worldgen/processor_list/{}.json", id.path()));
            let json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let expected = json["processors"].as_array().map_or_else(|| json.as_array().unwrap().len(), Vec::len);
            assert_eq!(holder.value().list().len(), expected, "{id}");
        }
    });
}

#[test]
fn processor_list_forms() {
    with_ctx(|ctx| {
        let one = json!({"processor_type": "minecraft:nop"});
        assert_eq!(decode_direct_list(&json!({"processors": [one.clone(), one.clone()]}), ctx).unwrap().list().len(), 2);
        assert_eq!(decode_direct_list(&json!([one.clone()]), ctx).unwrap().list().len(), 1);
        assert_eq!(decode_direct_list(&json!({"processors": []}), ctx).unwrap().list().len(), 0);
        let err = decode_direct_list(&json!({"processors": [{"processor_type": "minecraft:bogus"}]}), ctx).err().unwrap().to_string();
        assert!(err.starts_with("Failed to parse either. First: processors[0].processor_type: Unknown registry key in dispatch: minecraft:bogus; Second: "), "{err}");
        assert!(decode_direct_list(&json!("minecraft:empty"), ctx).is_err());
        // a `processor_list` holder is a reference for a string and an inline list otherwise
        assert!(ctx.processor_list(&json!("minecraft:empty")).unwrap().id().is_some());
        assert!(ctx.processor_list(&json!("minecraft:does_not_exist")).is_err());
        assert!(ctx.processor_list(&json!({"processors": []})).unwrap().id().is_none());
    });
}

#[test]
fn rule_tests() {
    with_ctx(|ctx| {
        let mut rng = random::create(1);
        let pos = BlockPos::new(3, 70, 5);
        let t = |v: Value| rule_test::decode(&v, ctx).unwrap();
        assert!(t(json!({"predicate_type": "minecraft:always_true"})).test(state("minecraft:dirt"), pos, &mut rng));
        let block = t(json!({"predicate_type": "minecraft:block_match", "block": "minecraft:stone"}));
        assert!(block.test(state("minecraft:stone"), pos, &mut rng) && !block.test(state("minecraft:dirt"), pos, &mut rng));
        let blockstate = t(json!({"predicate_type": "minecraft:blockstate_match", "block_state": {"id": "minecraft:oak_log", "properties": {"axis": "x"}}}));
        let x_log = Block::by_name("minecraft:oak_log").unwrap().default_state().with_property_str("axis", "x").unwrap();
        assert!(blockstate.test(x_log, pos, &mut rng) && !blockstate.test(state("minecraft:oak_log"), pos, &mut rng));
        let tag = t(json!({"predicate_type": "minecraft:tag_match", "tag": "minecraft:logs"}));
        assert!(tag.test(state("minecraft:oak_log"), pos, &mut rng) && !tag.test(state("minecraft:stone"), pos, &mut rng));
        let height = t(json!({"predicate_type": "minecraft:height_match", "min_inclusive": 70, "max_inclusive": 71}));
        assert!(height.test(state("minecraft:stone"), pos, &mut rng) && !height.test(state("minecraft:stone"), BlockPos::new(0, 72, 0), &mut rng));
        // the random is only drawn when the block matches (`&&` short circuit), like vanilla
        let random_block = t(json!({"predicate_type": "minecraft:random_block_match", "block": "minecraft:stone", "probability": 1.0}));
        let mut a = random::create(7);
        let mut b = random::create(7);
        assert!(!random_block.test(state("minecraft:dirt"), pos, &mut a));
        assert_eq!(a.next_int(), b.next_int());
        assert!(random_block.test(state("minecraft:stone"), pos, &mut a));
        b.next_float();
        assert_eq!(a.next_int(), b.next_int());
        let not = t(json!({"predicate_type": "minecraft:not", "rule": {"predicate_type": "minecraft:block_match", "block": "minecraft:stone"}}));
        assert!(!not.test(state("minecraft:stone"), pos, &mut rng));
        let any = t(json!({"predicate_type": "minecraft:any_of", "rules": [
            {"predicate_type": "minecraft:block_match", "block": "minecraft:stone"}, {"predicate_type": "minecraft:block_match", "block": "minecraft:dirt"}]}));
        assert!(any.test(state("minecraft:dirt"), pos, &mut rng) && !any.test(state("minecraft:sand"), pos, &mut rng));
        let all = t(json!({"predicate_type": "minecraft:all_of", "rules": [
            {"predicate_type": "minecraft:block_match", "block": "minecraft:stone"}, {"predicate_type": "minecraft:always_true"}]}));
        assert!(all.test(state("minecraft:stone"), pos, &mut rng) && !all.test(state("minecraft:dirt"), pos, &mut rng));
    });
}

#[test]
fn rule_test_rejects() {
    assert_eq!(rule_err(json!({"predicate_type": "minecraft:nope"})), "predicate_type: Unknown registry key in dispatch: minecraft:nope");
    assert_eq!(rule_err(json!({"predicate_type": "other:block_match", "block": "minecraft:stone"})), "predicate_type: Unknown registry key in dispatch: other:block_match");
    assert_eq!(rule_err(json!({"block": "minecraft:stone"})), "predicate_type: missing required field");
    assert_eq!(rule_err(json!({"type": "minecraft:always_true"})), "predicate_type: missing required field");
    assert_eq!(rule_err(json!({"predicate_type": "minecraft:block_match"})), "block: missing required field");
    assert!(rule_err(json!({"predicate_type": "minecraft:block_match", "block": "minecraft:not_a_block"})).starts_with("block: Unknown registry key"));
    assert_eq!(rule_err(json!({"predicate_type": "minecraft:random_block_match", "block": "minecraft:stone"})), "probability: missing required field");
    assert!(rule_err(json!({"predicate_type": "minecraft:all_of", "rules": [{"predicate_type": "minecraft:nope"}]})).starts_with("rules[0].predicate_type: "));
    assert_eq!(rule_err(json!({"predicate_type": "minecraft:not"})), "rule: missing required field");
    assert_eq!(rule_err(json!({"predicate_type": "minecraft:height_match", "min_inclusive": 1})), "max_inclusive: missing required field");
    assert_eq!(rule_err(json!("minecraft:always_true")), "expected object");
}

#[test]
fn pos_rule_tests() {
    with_ctx(|ctx| {
        let t = |v: Value| pos_rule_test::decode(&v, ctx);
        assert!(t(json!({"predicate_type": "minecraft:always_true"})).is_ok());
        assert!(t(json!({"predicate_type": "minecraft:linear_pos", "min_dist": 0, "max_dist": 4, "min_chance": 0.0, "max_chance": 1.0})).is_ok());
        assert!(t(json!({"predicate_type": "minecraft:axis_aligned_linear_pos", "axis": "x", "min_dist": 0, "max_dist": 4})).is_ok());
        // `new LinearPosTest(..)` throws IllegalArgumentException for an empty range (defaults 0/0 included)
        for ty in ["linear_pos", "axis_aligned_linear_pos"] {
            let e = t(json!({"predicate_type": format!("minecraft:{ty}")})).err().unwrap().to_string();
            assert_eq!(e, "Invalid range: [0,0]");
            assert!(t(json!({"predicate_type": format!("minecraft:{ty}"), "min_dist": 5, "max_dist": 2})).is_err());
        }
        let e = t(json!({"predicate_type": "minecraft:axis_aligned_linear_pos", "axis": "w", "min_dist": 0, "max_dist": 1})).err().unwrap().to_string();
        assert_eq!(e, "axis: Unknown element name:w");
        // distance 6 on the configured axis is past `max_dist`: the chance is `max_chance` (1.0 -> always true)
        let p = t(json!({"predicate_type": "minecraft:axis_aligned_linear_pos", "axis": "y", "min_dist": 0, "max_dist": 4, "min_chance": 0.0, "max_chance": 1.0})).unwrap();
        let mut rng = random::create(3);
        assert!(p.test(BlockPos::ZERO, BlockPos::new(100, 6, 100), BlockPos::ZERO, &mut rng));
        let q = t(json!({"predicate_type": "minecraft:linear_pos", "min_dist": 0, "max_dist": 4, "min_chance": 0.0, "max_chance": 0.0})).unwrap();
        // chance 0.0: `rnd <= 0.0` only for an exact 0 draw, never for this seed
        assert!(!q.test(BlockPos::ZERO, BlockPos::new(100, 6, 100), BlockPos::ZERO, &mut rng));
    });
}

#[test]
fn block_entity_modifiers() {
    with_ctx(|ctx| {
        let t = |v: Value| rule_block_entity_modifier::decode(&v, ctx);
        let mut rng = random::create(5);
        let mut existing = mc_nbt::CompoundTag::new();
        existing.put_int("a", 1);
        existing.put_int("b", 1);
        assert_eq!(t(json!({"type": "minecraft:passthrough"})).unwrap().apply(&mut rng, Some(&existing)), Some(existing.clone()));
        assert_eq!(t(json!({"type": "minecraft:passthrough"})).unwrap().apply(&mut rng, None), None);
        assert_eq!(t(json!({"type": "minecraft:clear"})).unwrap().apply(&mut rng, Some(&existing)), Some(mc_nbt::CompoundTag::new()));
        let appended = t(json!({"type": "minecraft:append_static", "data": {"b": 2, "c": "x", "d": 1.5}})).unwrap().apply(&mut rng, Some(&existing)).unwrap();
        assert_eq!((appended.get_int("a"), appended.get_int("b"), appended.get_string("c")), (Some(1), Some(2), Some("x")));
        // `JsonOps.convertTo`: 1.5 is exactly a float, 2 a byte
        assert_eq!(appended.get("d"), Some(&mc_nbt::Tag::Float(1.5)));
        assert_eq!(appended.get("b"), Some(&mc_nbt::Tag::Byte(2)));
        assert_eq!(t(json!({"type": "minecraft:append_static", "data": {"n": 300}})).unwrap().apply(&mut rng, None).unwrap().get_short("n"), Some(300));
        assert!(t(json!({"type": "minecraft:append_static", "data": [1]})).is_err());
        assert!(t(json!({"type": "minecraft:append_static"})).is_err());
        let mut a = random::create(9);
        let mut b = random::create(9);
        let loot = t(json!({"type": "minecraft:append_loot", "loot_table": "minecraft:chests/foo"})).unwrap().apply(&mut a, Some(&existing)).unwrap();
        assert_eq!((loot.get_string("LootTable"), loot.get_long("LootTableSeed"), loot.get_int("a")), (Some("minecraft:chests/foo"), Some(b.next_long()), Some(1)));
        assert!(t(json!({"type": "minecraft:append_loot"})).is_err());
        assert!(t(json!({"type": "minecraft:nope"})).is_err());
    });
}

#[test]
fn processor_rejects() {
    assert_eq!(processor_err(json!({"processor_type": "minecraft:nope"})), "processor_type: Unknown registry key in dispatch: minecraft:nope");
    assert_eq!(processor_err(json!({"type": "minecraft:nop"})), "processor_type: missing required field");
    assert_eq!(processor_err(json!({"processor_type": "minecraft:block_rot"})), "integrity: missing required field");
    assert_eq!(processor_err(json!({"processor_type": "minecraft:block_rot", "integrity": 1.5})), "integrity: Value 1.5 outside of range [0.0:1.0]");
    assert!(processor_err(json!({"processor_type": "minecraft:block_rot", "integrity": -0.1})).contains("outside of range"));
    assert_eq!(processor_err(json!({"processor_type": "minecraft:rule"})), "rules: missing required field");
    assert!(processor_err(json!({"processor_type": "minecraft:rule", "rules": [{"input_predicate": {"predicate_type": "minecraft:always_true"}}]})).starts_with("rules[0].location_predicate"));
    assert_eq!(processor_err(json!({"processor_type": "minecraft:protected_blocks"})), "value: missing required field");
    assert_eq!(processor_err(json!({"processor_type": "minecraft:capped", "limit": 1})), "delegate: missing required field");
    assert!(processor_err(json!({"processor_type": "minecraft:capped", "delegate": {"processor_type": "minecraft:nop"}, "limit": 0})).starts_with("limit: Value provider too low"));
    assert_eq!(processor_err(json!({"processor_type": "minecraft:block_age"})), "mossiness: missing required field");
    assert!(processor_err(json!({"processor_type": "minecraft:gravity", "heightmap": "world_surface_wg"})).starts_with("heightmap: Unknown element name"));
    assert_eq!(processor_err(json!({"processor_type": "minecraft:block_ignore"})), "blocks: missing required field");
}

#[test]
fn processor_accepts() {
    with_ctx(|ctx| {
        let t = |v: Value| decode_single(&v, ctx);
        for v in [
            json!({"processor_type": "minecraft:blackstone_replace"}),
            json!({"processor_type": "minecraft:lava_submerged_block"}),
            json!({"processor_type": "minecraft:jigsaw_replacement"}),
            json!({"processor_type": "minecraft:nop"}),
            json!({"processor_type": "minecraft:gravity"}),
            json!({"processor_type": "minecraft:gravity", "heightmap": "OCEAN_FLOOR", "offset": -3}),
            json!({"processor_type": "minecraft:block_age", "mossiness": 0.5}),
            json!({"processor_type": "minecraft:block_rot", "integrity": 0.5}),
            json!({"processor_type": "minecraft:block_rot", "integrity": 1.0, "rottable_blocks": "#minecraft:logs"}),
            json!({"processor_type": "minecraft:block_rot", "integrity": 0.0, "rottable_blocks": ["minecraft:stone"]}),
            json!({"processor_type": "minecraft:block_ignore", "blocks": [{"id": "minecraft:air"}, "minecraft:structure_block"]}),
            json!({"processor_type": "minecraft:protected_blocks", "value": "#minecraft:features_cannot_replace"}),
            json!({"processor_type": "minecraft:capped", "limit": {"type": "minecraft:uniform", "min_inclusive": 1, "max_inclusive": 3}, "delegate": {"processor_type": "minecraft:nop"}}),
        ] {
            assert!(t(v.clone()).is_ok(), "{v}");
        }
        // lenient optional fields of a rule fall back to their defaults instead of failing
        let rule = json!({"processor_type": "minecraft:rule", "rules": [{
            "input_predicate": {"predicate_type": "minecraft:always_true"},
            "location_predicate": {"predicate_type": "minecraft:always_true"},
            "position_predicate": {"predicate_type": "minecraft:bogus"},
            "block_entity_modifier": 5,
            "output_state": "minecraft:stone"}]});
        assert!(t(rule).is_ok());
    });
}
