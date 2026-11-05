//! `CreakingHeartBlockEntity`: saves the protector's UUID (`creaking`) and sends it to clients (update tag/packet).
//!
//! shortcut: the protector is only tracked by UUID (vanilla's `Either<Creaking, UUID>`); the creaking entity link and
//! the server tick (spawning, particles, analog output) are not ported yet.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use mc_protocol::components::uuid_codec;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;
use uuid::Uuid;

pub struct CreakingHeartBlockEntity {
    base: BlockEntityData,
    creaking_info: Option<Uuid>,
    ticks_existed: i64,
    output_signal: i32,
}

impl CreakingHeartBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::CREAKING_HEART, pos, state), creaking_info: None, ticks_existed: 0, output_signal: 0 }
    }

    /// `getAnalogOutputSignal()`.
    pub fn get_analog_output_signal(&self) -> i32 {
        self.output_signal
    }

    /// `setCreakingInfo(uuid)`.
    pub fn set_creaking_info(&mut self, uuid: Uuid) {
        self.creaking_info = Some(uuid);
        self.ticks_existed = 0;
    }
}

impl BlockEntity for CreakingHeartBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        match read_codec(tag, "creaking", &uuid_codec()) {
            Some(uuid) => self.set_creaking_info(uuid),
            None => self.creaking_info = None,
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(uuid) = &self.creaking_info
            && let Ok(encoded) = mc_dfu::encode_nbt(&uuid_codec(), uuid).into_result()
        {
            tag.put("creaking", encoded);
        }
    }

    /// `getUpdateTag`: `saveCustomOnly`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CREAKING_HEART: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::CREAKING_HEART, create: |pos, state| Box::new(CreakingHeartBlockEntity::new(pos, state)) };
