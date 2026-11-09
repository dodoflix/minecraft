//! Goldens of `DecoratedPotBlock` (+ block entity), `StrawBedBlock.spawnDestroyParticles` and `SegmentableBlock` (pink petals,
//! wildflowers, leaf litter): scenarios `tests/oracle/scenarios/blk_{decorated_pot,straw_bed_destroy,segment_blocks}.txt` replayed
//! on the real vanilla level (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/NAME.txt`),
//! plus the `TestBlock` placement rule.

use crate::common;
use mc_core::direction::Direction;
use mc_core::ident::Identifier;
use mc_core::phys::{BlockHitResult, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::enums::TestBlockMode;
use mc_data::{Item, props};
use mc_game::server::level::ServerLevel;
use mc_game::world::item::{BlockPlaceContext, InteractionHand, ItemStack, UseOnContext};
use mc_game::world::level::BlockGetter;
use mc_game::world::level::block::helpers;
use mc_protocol::components::values::containers::{BLOCK_STATE, BlockItemStateProperties};

#[test]
fn decorated_pot_matches_vanilla() {
    common::block_scenario::check("blk_decorated_pot");
}

#[test]
fn straw_bed_destroy_particles_match_vanilla() {
    common::block_scenario::check("blk_straw_bed_destroy");
}

#[test]
fn segmentable_blocks_match_vanilla() {
    common::block_scenario::check("blk_segment_blocks");
}

/// `TestBlock.getStateForPlacement`: the `mode` of the item's `block_state` component, the default state when it is absent or
/// not a mode. Placing the item needs the game master permission, which the block oracle's players do not have, so the
/// expectation comes from reading the vanilla class instead of a recording.
#[test]
fn test_block_item_places_the_mode_of_its_block_state() {
    let mut level = ServerLevel::detached(Identifier::parse("minecraft:overworld").unwrap());
    let hit = BlockHitResult {
        location: Vec3::new(0.5, 1.0, 0.5),
        direction: Direction::Up,
        block_pos: BlockPos::new(0, 0, 0),
        miss: false,
        inside: false,
        world_border_hit: false,
    };
    let placed_mode = |level: &mut ServerLevel, stack: ItemStack| {
        let use_on = UseOnContext::new(None, InteractionHand::MainHand, stack, hit);
        let context = BlockPlaceContext::new(use_on, |pos| level.get_block_state(pos));
        helpers::get_state_for_placement(Block::TEST_BLOCK, level, &context)
            .expect("a test block always has a placement state")
            .get(props::TEST_BLOCK_MODE)
    };
    let with_mode = |mode: &str| {
        let mut stack = ItemStack::new(Item::TEST_BLOCK);
        stack.set(
            BLOCK_STATE,
            BlockItemStateProperties {
                properties: vec![("mode".to_owned(), mode.to_owned())],
            },
        );
        stack
    };
    for (name, mode) in [
        ("start", TestBlockMode::Start),
        ("log", TestBlockMode::Log),
        ("fail", TestBlockMode::Fail),
        ("accept", TestBlockMode::Accept),
    ] {
        assert_eq!(placed_mode(&mut level, with_mode(name)), mode, "{name}");
    }
    assert_eq!(
        placed_mode(&mut level, with_mode("not_a_mode")),
        TestBlockMode::Start
    );
    assert_eq!(
        placed_mode(&mut level, ItemStack::new(Item::TEST_BLOCK)),
        TestBlockMode::Start
    );
}
