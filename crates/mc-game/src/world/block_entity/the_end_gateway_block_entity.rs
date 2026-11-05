//! `TheEndGatewayBlockEntity`: `Age`, `exit_portal` and `ExactTeleport`, sent to clients as update tag/packet. The
//! teleport cooldown/exit search logic is not ported yet.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, read_codec};
use crate::commands::arguments::coordinates::block_pos_argument::is_in_spawnable_bounds;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use mc_protocol::components::block_pos_codec;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

pub struct TheEndGatewayBlockEntity {
    base: BlockEntityData,
    pub age: i64,
    pub teleport_cooldown: i32,
    pub exit_portal: Option<BlockPos>,
    pub exact_teleport: bool,
}

impl TheEndGatewayBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::END_GATEWAY, pos, state), age: 0, teleport_cooldown: 0, exit_portal: None, exact_teleport: false }
    }

    /// `setExitPosition(exactPosition, exact)`.
    pub fn set_exit_position(&mut self, exact_position: BlockPos, exact: bool) {
        self.exact_teleport = exact;
        self.exit_portal = Some(exact_position);
    }
}

impl BlockEntity for TheEndGatewayBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.age = tag.get_long_or("Age", 0);
        self.exit_portal = read_codec(tag, "exit_portal", &block_pos_codec()).filter(|p| is_in_spawnable_bounds(*p));
        self.exact_teleport = tag.get_boolean_or("ExactTeleport", false);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_long("Age", self.age);
        if let Some(exit) = &self.exit_portal
            && let Ok(encoded) = mc_dfu::encode_nbt(&block_pos_codec(), exit).into_result()
        {
            tag.put("exit_portal", encoded);
        }
        if self.exact_teleport {
            tag.put_boolean("ExactTeleport", true);
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
static END_GATEWAY: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::END_GATEWAY, create: |pos, state| Box::new(TheEndGatewayBlockEntity::new(pos, state)) };
