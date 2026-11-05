//! `treedecorators.BeehiveDecorator`.

use super::TREE_DECORATOR_TYPES;
use mc_game_core::world::level::levelgen::feature::tree::shuffle;
use super::tree_decorator::{Context, TreeDecorator};
use mc_game_core::world::level::levelgen::block_entity_data::base_tag;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_float_range};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::props;
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::sync::Arc;

const WORLDGEN_FACING: Direction = Direction::South;
/// The horizontal directions except `WORLDGEN_FACING.getOpposite()`, in `Direction.Plane.HORIZONTAL` order.
const SPAWN_DIRECTIONS: [Direction; 3] = [Direction::East, Direction::South, Direction::West];

pub struct BeehiveDecorator {
    probability: f32,
}

/// The saved form of a fresh `BeehiveBlockEntity` holding `Occupant.create(ticks_in_hive)` bees.
fn beehive_block_entity_nbt(pos: BlockPos, ticks_in_hive: &[i32]) -> CompoundTag {
    let mut bees = ListTag::new();
    for &ticks in ticks_in_hive {
        let mut entity_data = CompoundTag::new();
        entity_data.put_string("id", "minecraft:bee");
        let mut bee = CompoundTag::new();
        bee.put("entity_data", Tag::Compound(entity_data));
        bee.put_int("min_ticks_in_hive", 600);
        bee.put_int("ticks_in_hive", ticks);
        bees.push(Tag::Compound(bee));
    }
    let mut tag = base_tag("minecraft:beehive", pos);
    tag.put("bees", Tag::List(bees));
    tag
}

impl TreeDecorator for BeehiveDecorator {
    fn place(&self, context: &mut Context<'_>) {
        let leaves = context.leaves.clone();
        let logs = context.logs.clone();
        if logs.is_empty() || context.random.next_float() >= self.probability {
            return;
        }
        let hive_y = if !leaves.is_empty() {
            (leaves[0].y() - 1).max(logs[0].y() + 1)
        } else {
            (logs[0].y() + 1 + context.random.next_int_bounded(3)).min(logs[logs.len() - 1].y())
        };
        let mut hive_placements: Vec<BlockPos> =
            logs.iter().filter(|pos| pos.y() == hive_y).flat_map(|pos| SPAWN_DIRECTIONS.iter().map(move |dir| pos.relative(*dir))).collect();
        if hive_placements.is_empty() {
            return;
        }
        shuffle(&mut hive_placements, context.random);
        let Some(hive_pos) = hive_placements.into_iter().find(|pos| context.is_air(*pos) && context.is_air(pos.relative(WORLDGEN_FACING))) else { return };
        context.set_block(hive_pos, Block::BEE_NEST.default_state().set(props::HORIZONTAL_FACING, WORLDGEN_FACING));
        if context.level.get_block_entity_nbt(hive_pos).is_some() {
            let num_bees = 2 + context.random.next_int_bounded(2);
            let ticks: Vec<i32> = (0..num_bees).map(|_| context.random.next_int_bounded(599)).collect();
            context.level.set_block_entity_nbt(beehive_block_entity_nbt(hive_pos, &ticks));
        }
    }
}

#[linkme::distributed_slice(TREE_DECORATOR_TYPES)]
static BEEHIVE: TypeEntry<dyn TreeDecorator> = TypeEntry {
    name: "beehive",
    decode: |fields, _| Ok(Arc::new(BeehiveDecorator { probability: fields.required("probability", |v| parse_float_range(v, 0.0, 1.0))? })),
};
