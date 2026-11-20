//! The order the keys of an encoded block state's `properties` compound are written in, against the real vanilla server
//! (`oracle/BlockStateOrderOracle.java`, regenerate with `scripts/entity-oracle.sh`; line format in its header). The order
//! of colliding keys in the compound's hash map depends on the order they were inserted in, so every encoder of
//! `BlockState.CODEC` must insert them the way the nested `Codec.mapPair`s of `StateDefinition.createCodec` do.

use mc_core::ident::Identifier;
use mc_data::block::{Block, BlockState};
use mc_data::enums::EntitySpawnReason;
use mc_dfu::{Codec, encode_nbt};
use mc_game::server::level::ServerLevel;
use mc_game::world::entity::entity::save;
use mc_game::world::entity::factory::load_entities_recursive;
use mc_game::world::item::enchantment::value_codecs;
use mc_nbt::{CompoundTag, Tag, snbt};
use mc_world::palette::{BlockStateCodec, ElementCodec};

const FIXTURE: &str = include_str!("../fixtures/block_state_order.txt");

fn keys(tag: &Tag) -> String {
    let properties = tag
        .as_compound()
        .and_then(|c| c.get_compound("properties"))
        .unwrap_or_else(|| panic!("no properties in {tag:?}"));
    properties
        .iter_java_order()
        .into_iter()
        .map(|(key, _)| key.as_lossy_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn encoded(codec: &Codec<BlockState>, state: BlockState) -> String {
    keys(&encode_nbt(codec, &state).into_result().expect("encodes"))
}

#[test]
fn block_state_codecs_write_properties_in_vanilla_key_order() {
    let protocol = mc_protocol::packets::play::block_state_codec();
    let core = value_codecs::block_state_codec();
    let full = value_codecs::block_state_full_codec();
    let (mut codec_lines, mut full_lines, mut entity_lines) = (0, 0, 0);
    let level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    for line in FIXTURE.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        match fields[0] {
            "CODEC" | "FULL" => {
                let block = Block::by_name(fields[1]).expect("block");
                let state = block.states().last().expect("a state");
                let state = if state == block.default_state() {
                    block.states().next().expect("a state")
                } else {
                    state
                };
                let expected = fields[2];
                if fields[0] == "CODEC" {
                    assert_eq!(encoded(&protocol, state), expected, "protocol {line}");
                    assert_eq!(encoded(&core, state), expected, "game-core {line}");
                    let palette = BlockStateCodec.encode(state).expect("palette");
                    assert_eq!(keys(&palette), expected, "palette {line}");
                    codec_lines += 1;
                } else {
                    assert_eq!(encoded(&full, state), expected, "full {line}");
                    full_lines += 1;
                }
            }
            "ENTITY" => {
                let (entity, field, state, expected) = (fields[1], fields[2], fields[3], fields[4]);
                let tag = snbt::parse(&format!(
                    "{{id:\"{entity}\",Pos:[0.5d,-50.0d,0.5d],{field}:{state}}}"
                ))
                .expect("snbt");
                let tag = tag.as_compound().expect("compound");
                let entities = load_entities_recursive(
                    std::slice::from_ref(tag),
                    &level,
                    EntitySpawnReason::Load,
                );
                assert_eq!(entities.len(), 1, "{line}");
                let saved: CompoundTag = save(entities[0].as_ref(), &|_| None).expect("saves");
                assert_eq!(keys(saved.get(field).expect(field)), expected, "{line}");
                entity_lines += 1;
            }
            other => panic!("unknown fixture line kind {other}"),
        }
    }
    assert!(codec_lines > 100 && codec_lines == full_lines && entity_lines == 13);
}
