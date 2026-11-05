//! `net.minecraft.world.level.block.SpawnerBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;

pub struct SpawnerBlock;

impl BlockBehaviour for SpawnerBlock {
    fn trigger_event(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool {
        helpers::base_entity_trigger_event(level, pos, b0, b1)
    }

    /// 15 + two `nextInt(15)` experience when broken with a tool that drops experience.
    fn spawn_after_break(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, _tool: &ItemStack, drop_experience: bool) {
        if drop_experience {
            let random = level.random();
            let magic_count = 15 + random.next_int_bounded(15) + random.next_int_bounded(15);
            helpers::pop_experience(level, pos, magic_count);
        }
    }
}

register_block_class!("SpawnerBlock", |_| Box::new(SpawnerBlock));
