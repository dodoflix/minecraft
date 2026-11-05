//! `BlockBasedTestInstance.run`: test blocks drive the test (the start block powers it, accept/fail/log blocks end it).

use super::game_test_helper::{GameTestHelper, TestResult};
use crate::translatable;
use crate::world::block_entity::TestBlockEntity;
use crate::world::block_entity::test_block_entity;
use crate::world::level::block::BlockStateExt;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::enums::TestBlockMode;
use mc_data::props;
use mc_text::Component;

/// `TestBlockMode.getDisplayName()`.
pub fn mode_display_name(mode: TestBlockMode) -> Component {
    Component::translatable(format!("test_block.mode.{}", mode.serialized_name().expect("serialized")), Vec::new())
}

/// `run(helper)`.
pub fn run(helper: &mut GameTestHelper<'_>) -> TestResult {
    let start_pos = find_start_block(helper)?;
    helper.get_block_entity::<TestBlockEntity, _>(start_pos, |_| ())?;
    let absolute = helper.absolute_pos(start_pos);
    test_block_entity::trigger(helper.get_level(), absolute);
    helper.on_each_tick(|helper| {
        let accept_blocks = find_test_blocks(helper, TestBlockMode::Accept);
        if accept_blocks.is_empty() {
            return helper.fail(translatable!("test_block.error.missing", mode_display_name(TestBlockMode::Accept)));
        }
        let mut accept_triggered = false;
        for pos in &accept_blocks {
            if helper.get_block_entity::<TestBlockEntity, _>(*pos, |be| be.has_triggered())? {
                accept_triggered = true;
                break;
            }
        }
        if accept_triggered {
            helper.succeed();
            return Ok(());
        }
        for_all_triggered_test_blocks(helper, TestBlockMode::Fail, |helper, message, _| helper.fail(Component::literal(message)))?;
        for_all_triggered_test_blocks(helper, TestBlockMode::Log, |helper, _, pos| {
            let absolute = helper.absolute_pos(pos);
            test_block_entity::trigger(helper.get_level(), absolute);
            Ok(())
        })
    });
    Ok(())
}

/// `forAllTriggeredTestBlocks(helper, mode, action)`: `action` gets the entity's message and the block's position.
fn for_all_triggered_test_blocks(
    helper: &mut GameTestHelper<'_>,
    mode: TestBlockMode,
    action: impl Fn(&mut GameTestHelper<'_>, String, BlockPos) -> TestResult,
) -> TestResult {
    for pos in find_test_blocks(helper, mode) {
        let (triggered, message) = helper.get_block_entity::<TestBlockEntity, _>(pos, |be| (be.has_triggered(), be.get_message().to_owned()))?;
        if triggered {
            action(helper, message, pos)?;
            let absolute = helper.absolute_pos(pos);
            test_block_entity::reset(helper.get_level(), absolute);
        }
    }
    Ok(())
}

/// `findStartBlock(helper)`.
fn find_start_block(helper: &mut GameTestHelper<'_>) -> Result<BlockPos, super::game_test_exception::GameTestException> {
    let test_blocks = find_test_blocks(helper, TestBlockMode::Start);
    if test_blocks.is_empty() {
        helper.fail(translatable!("test_block.error.missing", mode_display_name(TestBlockMode::Start)))?;
    }
    if test_blocks.len() != 1 {
        helper.fail(translatable!("test_block.error.too_many", mode_display_name(TestBlockMode::Start)))?;
    }
    Ok(test_blocks[0])
}

/// `findTestBlocks(helper, mode)`.
fn find_test_blocks(helper: &mut GameTestHelper<'_>, mode: TestBlockMode) -> Vec<BlockPos> {
    let mut blocks = Vec::new();
    helper.for_every_block_in_structure(|helper, pos| {
        let state = helper.get_block_state(pos);
        if state.is_block(Block::TEST_BLOCK) && state.get(props::TEST_BLOCK_MODE) == mode {
            blocks.push(pos);
        }
    });
    blocks
}
