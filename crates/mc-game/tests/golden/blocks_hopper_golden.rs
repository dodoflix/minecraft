//! Goldens of hoppers, droppers and dispensers: scenarios `tests/oracle/scenarios/blk_*.txt` replayed on the real vanilla level
//! (`scripts/block-oracle.sh NAME`) vs our ticking level (fixtures `tests/fixtures/entity_oracle/blk_*.txt`).

use crate::common;

/// Cooldown 8, the `tickedGameTime` skip, the redstone lock, full inventories, merging, item entities above a hopper.
#[test]
fn hopper_chains_cooldowns_and_item_entities_match_vanilla() {
    common::block_scenario::check("blk_hopper_chain");
}

/// Triggering, the random slot from the level seed, the default behaviour, the empty event 1001, droppers into containers.
#[test]
fn droppers_and_dispensers_match_vanilla() {
    common::block_scenario::check("blk_dropper_dispenser");
}

/// Pushing into and pulling out of chests, double chests, barrels, shulker boxes, brewing stands, crafters, shelves and the rest.
#[test]
fn hoppers_and_every_container_match_vanilla() {
    common::block_scenario::check("blk_hopper_containers");
}

/// `HopperBlock.getInteractionShape` (the bowl, plus the spout of a horizontal facing) against the extracted vanilla shapes.
#[test]
fn hopper_interaction_shape_matches_vanilla() {
    use mc_core::pos::BlockPos;
    use mc_data::block::Block;
    use mc_game::world::block_entity::BlockEntity;
    use mc_game::world::level::block::BlockStateExt;
    use mc_game::world::level::shapes::voxel_shape;
    use mc_game::world::level::{BlockGetter, LevelHeight};

    struct Empty;
    impl BlockGetter for Empty {
        fn get_block_state(&self, _pos: BlockPos) -> mc_data::block::BlockState {
            Block::AIR.default_state()
        }
        fn get_fluid_state(&self, _pos: BlockPos) -> mc_data::fluid::FluidState {
            Block::AIR.default_state().get_fluid_state()
        }
        fn get_block_entity(&self, _pos: BlockPos) -> Option<&dyn BlockEntity> {
            None
        }
        fn level_height(&self) -> LevelHeight {
            LevelHeight::new(-64, 384)
        }
    }
    let mut states = 0;
    for state in Block::HOPPER.states() {
        let expected = voxel_shape(state.interaction_shape());
        assert!(
            !expected.is_empty(),
            "{state:?}: the extracted shape is the bowl"
        );
        assert!(
            state.get_interaction_shape(&Empty, BlockPos::new(0, 0, 0)) == expected,
            "{state:?}"
        );
        states += 1;
    }
    assert_eq!(states, 10);
}
