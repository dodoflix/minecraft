//! `JigsawBlockEntity`: name / target / pool / joint / final state / priorities of a jigsaw block (`generate` is in `mc-game-levelgen`, which owns the placement).

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::world::level::levelgen::structure::templatesystem::structure_template::{
    JointType, get_default_joint_type,
};
use linkme::distributed_slice;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

/// `JigsawBlockEntity.EMPTY_ID` (`Pools.EMPTY` is the template pool key with the same id).
fn empty_id() -> Identifier {
    Identifier::parse("minecraft:empty").expect("valid identifier")
}

pub struct JigsawBlockEntity {
    base: BlockEntityData,
    name: Identifier,
    target: Identifier,
    /// `ResourceKey<StructureTemplatePool>`: the id of the pool.
    pool: Identifier,
    joint: JointType,
    final_state: String,
    placement_priority: i32,
    selection_priority: i32,
}

impl JigsawBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::JIGSAW, pos, state),
            name: empty_id(),
            target: empty_id(),
            pool: empty_id(),
            joint: JointType::Rollable,
            final_state: "minecraft:air".to_owned(),
            placement_priority: 0,
            selection_priority: 0,
        }
    }

    pub fn get_name(&self) -> &Identifier {
        &self.name
    }

    pub fn get_target(&self) -> &Identifier {
        &self.target
    }

    pub fn get_pool(&self) -> &Identifier {
        &self.pool
    }

    pub fn get_final_state(&self) -> &str {
        &self.final_state
    }

    pub fn get_joint(&self) -> JointType {
        self.joint
    }

    pub fn get_placement_priority(&self) -> i32 {
        self.placement_priority
    }

    pub fn get_selection_priority(&self) -> i32 {
        self.selection_priority
    }

    pub fn set_name(&mut self, name: Identifier) {
        self.name = name;
    }

    pub fn set_target(&mut self, target: Identifier) {
        self.target = target;
    }

    pub fn set_pool(&mut self, pool: Identifier) {
        self.pool = pool;
    }

    pub fn set_final_state(&mut self, final_state: &str) {
        self.final_state = final_state.to_owned();
    }

    pub fn set_joint(&mut self, joint: JointType) {
        self.joint = joint;
    }

    pub fn set_placement_priority(&mut self, placement_priority: i32) {
        self.placement_priority = placement_priority;
    }

    pub fn set_selection_priority(&mut self, selection_priority: i32) {
        self.selection_priority = selection_priority;
    }
}

/// `input.read(key, Identifier.CODEC)`.
fn read_identifier(tag: &CompoundTag, key: &str) -> Option<Identifier> {
    tag.get(key)?.as_string().and_then(Identifier::try_parse)
}

impl BlockEntity for JigsawBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.name = read_identifier(tag, "name").unwrap_or_else(empty_id);
        self.target = read_identifier(tag, "target").unwrap_or_else(empty_id);
        self.pool = read_identifier(tag, "pool").unwrap_or_else(empty_id);
        self.final_state = tag.get_string_or("final_state", "minecraft:air").to_owned();
        self.joint = tag
            .get("joint")
            .and_then(|t| t.as_string())
            .and_then(JointType::by_name)
            .unwrap_or_else(|| get_default_joint_type(self.get_block_state()));
        self.placement_priority = tag.get_int_or("placement_priority", 0);
        self.selection_priority = tag.get_int_or("selection_priority", 0);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        tag.put_string("name", self.name.to_string());
        tag.put_string("target", self.target.to_string());
        tag.put_string("pool", self.pool.to_string());
        tag.put_string("final_state", self.final_state.clone());
        tag.put_string("joint", self.joint.get_serialized_name());
        tag.put_int("placement_priority", self.placement_priority);
        tag.put_int("selection_priority", self.selection_priority);
    }

    /// `getUpdateTag`: `saveCustomOnly`.
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
static JIGSAW: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::JIGSAW,
    create: |pos, state| Box::new(JigsawBlockEntity::new(pos, state)),
};
