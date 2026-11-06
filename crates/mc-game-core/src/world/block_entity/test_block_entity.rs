//! `TestBlockEntity`: mode, message and trigger state of a test block. The level-dependent operations (`reset`,
//! `trigger`, `setMode`) are free functions over `(level, pos)`: they read/write the entity in place instead of taking it
//! out of its chunk, because neighbor updates they cause read the entity again (`ownSignal`).

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::server::level::ServerLevel;
use crate::world::level::block::BlockStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::{Block, BlockState};
use mc_data::enums::TestBlockMode;
use mc_data::props;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::ClientboundBlockEntityDataPacket;

pub struct TestBlockEntity {
    base: BlockEntityData,
    mode: TestBlockMode,
    message: String,
    powered: bool,
    triggered: bool,
}

impl TestBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::TEST_BLOCK, pos, state),
            mode: state.get(props::TEST_BLOCK_MODE),
            message: String::new(),
            powered: false,
            triggered: false,
        }
    }

    pub fn is_powered(&self) -> bool {
        self.powered
    }

    pub fn set_powered(&mut self, powered: bool) {
        self.powered = powered;
    }

    pub fn get_mode(&self) -> TestBlockMode {
        self.mode
    }

    pub fn has_triggered(&self) -> bool {
        self.triggered
    }

    pub fn get_message(&self) -> &str {
        &self.message
    }

    pub fn set_message(&mut self, message: String) {
        self.message = message;
    }

    /// `log()`.
    pub fn log(&self) {
        if !self.message.trim().is_empty() {
            log::info!(
                "Test {} (at {}): {}",
                self.mode.serialized_name().unwrap_or_default(),
                self.get_block_pos(),
                self.message
            );
        }
    }
}

impl BlockEntity for TestBlockEntity {
    block_entity_base!();

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_string("mode", self.mode.serialized_name().unwrap_or("fail"));
        tag.put_string("message", self.message.clone());
        tag.put_boolean("powered", self.powered);
    }

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.mode = tag
            .get_string("mode")
            .and_then(TestBlockMode::by_serialized_name)
            .unwrap_or(TestBlockMode::Fail);
        self.message = tag.get_string_or("message", "").to_owned();
        self.powered = tag.get_boolean_or("powered", false);
    }

    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.get_block_pos(),
            r#type: self.get_type(),
            tag: self.get_update_tag(),
        })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static TEST_BLOCK: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::TEST_BLOCK,
    create: |pos, state| Box::new(TestBlockEntity::new(pos, state)),
};

/// The test block entity at `pos`.
pub fn get(level: &mut ServerLevel, pos: BlockPos) -> Option<&mut TestBlockEntity> {
    level
        .get_block_entity_immediate(pos)?
        .as_any_mut()
        .downcast_mut::<TestBlockEntity>()
}

/// `reset()`.
pub fn reset(level: &mut ServerLevel, pos: BlockPos) {
    let Some(be) = get(level, pos) else { return };
    be.triggered = false;
    if be.mode == TestBlockMode::Start {
        be.powered = false;
        level.update_neighbors_at(pos, Block::TEST_BLOCK);
    }
}

/// `trigger()`.
pub fn trigger(level: &mut ServerLevel, pos: BlockPos) {
    let Some(be) = get(level, pos) else { return };
    if be.mode == TestBlockMode::Start {
        be.powered = true;
        level.update_neighbors_at(pos, Block::TEST_BLOCK);
        if let Some(be) = get(level, pos) {
            be.log();
        }
    } else {
        if be.mode == TestBlockMode::Log {
            be.log();
        }
        be.triggered = true;
    }
}

/// `setMode(mode)` + `updateBlockState()`.
pub fn set_mode(level: &mut ServerLevel, pos: BlockPos, mode: TestBlockMode) {
    let Some(be) = get(level, pos) else { return };
    be.mode = mode;
    let block_state = level.get_block_state(pos);
    if block_state.is_block(Block::TEST_BLOCK) {
        level.set_block(pos, block_state.set(props::TEST_BLOCK_MODE, mode), 2, 512);
    }
}
