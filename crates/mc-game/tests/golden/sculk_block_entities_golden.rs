//! Golden test of the worldgen block entities (sculk catalyst/sensor/calibrated sensor/shrieker, creaking heart, potent
//! sulfur, end portal/gateway) against the vanilla classes (`tests/oracle/SculkBlockEntitiesOracle.java`, fixture
//! `tests/fixtures/sculk_block_entities.txt`): the NBT of a fresh entity (`saveWithFullMetadata`) and of an entity loaded
//! from sample NBT (`loadWithComponents` then `saveWithFullMetadata`), the update tag and the update packet, byte for byte.

use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::Block;
use mc_game::world::block_entity::create_block_entity;
use mc_nbt::{CompoundTag, nbt_io};

const POS: BlockPos = BlockPos::new(10, 64, -20);

fn hex(tag: &CompoundTag) -> String {
    nbt_io::to_bytes(tag).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn block_entities_match_vanilla() {
    let mut failures = Vec::new();
    let mut cases = 0;
    for line in include_str!("../fixtures/sculk_block_entities.txt").lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let [name, block, props, input, saved_hex, saved_snbt, update_hex, has_packet, packet_hex] = f[..] else { panic!("bad line {line}") };
        let block = Block::by_name(block).unwrap();
        let props = props.split(',').filter(|p| !p.is_empty()).map(|p| p.split_once('=').unwrap());
        let state = block.state_from_properties(props).unwrap_or_else(|| panic!("{name}: bad state"));
        let ty = BlockEntityType::iter().find(|ty| ty.data().valid_blocks.contains(&block)).unwrap();
        let mut entity = create_block_entity(ty, POS, state).unwrap_or_else(|| panic!("{name}: {} has no implementation", ty.name()));
        if input != "-" {
            let mut tag = mc_nbt::snbt::parse_compound(input).unwrap_or_else(|e| panic!("{name}: {}", e.message()));
            tag.put_string("id", ty.name());
            tag.put_int("x", POS.x());
            tag.put_int("y", POS.y());
            tag.put_int("z", POS.z());
            entity.load_with_components(&tag);
        }
        let saved = entity.save_with_full_metadata();
        let mut diffs = Vec::new();
        if hex(&saved) != saved_hex {
            diffs.push(format!("saved\n  java: {saved_snbt}\n  rust: {saved:?}"));
        }
        if hex(&entity.get_update_tag()) != update_hex {
            diffs.push("update tag".to_string());
        }
        let packet = entity.get_update_packet();
        if packet.is_some() != (has_packet == "true") {
            diffs.push(format!("update packet present: java {has_packet}"));
        } else if let Some(packet) = packet
            && (hex(&packet.tag) != packet_hex || packet.pos != POS || packet.r#type != ty)
        {
            diffs.push("update packet".to_string());
        }
        if !diffs.is_empty() {
            failures.push(format!("{name}: {}", diffs.join("; ")));
        }
        cases += 1;
    }
    assert!(cases > 100, "fixture has {cases} cases");
    assert!(failures.is_empty(), "{} of {cases} cases differ:\n{}", failures.len(), failures.join("\n"));
}
