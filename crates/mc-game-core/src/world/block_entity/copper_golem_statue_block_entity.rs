//! `CopperGolemStatueBlockEntity`: keeps only the golem's custom name (as a component) and converts back to a golem.

use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, set_changed,
};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::item::ItemStack;
use crate::world::level::block::helpers;
use linkme::distributed_slice;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::{CopperGolemStatueBlockPose, EntitySpawnReason};
use mc_data::{BlockEntityType, DataComponentType, EntityType, props};
use mc_protocol::components::map::DataComponentGetter as _;
use mc_protocol::components::{CUSTOM_NAME, DataComponentMap, Value};
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;
use mc_text::Component;

pub struct CopperGolemStatueBlockEntity {
    base: BlockEntityData,
}

impl CopperGolemStatueBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::COPPER_GOLEM_STATUE, pos, state),
        }
    }

    /// `createStatue(copperGolem)`: stores the golem's custom name, then `BlockEntity.setChanged()`.
    pub fn create_statue(&mut self, level: &mut ServerLevel, custom_name: Option<Component>) {
        let mut builder = DataComponentMap::builder().add_all(&self.base.components);
        builder.set_value(DataComponentType::CUSTOM_NAME, custom_name.map(Value::new));
        self.base.components = builder.build();
        set_changed(level, self.base.pos, self.base.block_state);
    }

    /// `removeStatue(state)`: the golem the statue turns back into (`None` when the entity cannot be created). The
    /// caller plays `initCopperGolem`'s `playSpawnSound` through `Entity::copper_golem_play_spawn_sound`, which needs the
    /// mutable level this block entity borrows from.
    pub fn remove_statue(&self, level: &ServerLevel, state: BlockState) -> Option<Box<dyn Entity>> {
        let mut golem = factory::create(
            EntityType::COPPER_GOLEM,
            level,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Triggered,
                ignore_checks: false,
            },
        )?;
        golem
            .base_mut()
            .set_custom_name(self.base.components.get(&CUSTOM_NAME).cloned());
        let pos = self.base.pos;
        let center = Vec3::at_center_of(&pos);
        let y_rot = state.get(props::HORIZONTAL_FACING).to_y_rot();
        golem.snap_to(center.x, f64::from(pos.y()), center.z, y_rot, 0.0);
        let y_rot = golem.base().y_rot();
        golem.set_y_head_rot(y_rot);
        golem.set_y_body_rot(y_rot);
        Some(golem)
    }

    /// `getItem(itemStack, pose)`.
    pub fn get_item(&self, mut stack: ItemStack, pose: CopperGolemStatueBlockPose) -> ItemStack {
        for (ty, value) in self.collect_components().iter() {
            stack.set_value(ty, Some(value.clone()));
        }
        helpers::set_block_state_property(
            &mut stack,
            "copper_golem_pose",
            pose.serialized_name().unwrap_or_default(),
        );
        stack
    }
}

impl BlockEntity for CopperGolemStatueBlockEntity {
    block_entity_base!();

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket {
            pos: self.base.pos,
            r#type: self.base.ty,
            tag: self.get_update_tag(),
        })
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static COPPER_GOLEM_STATUE: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::COPPER_GOLEM_STATUE,
    create: |pos, state| Box::new(CopperGolemStatueBlockEntity::new(pos, state)),
};
