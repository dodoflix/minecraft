//! `net.minecraft.world.level.block.CactusBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;

const MAX_CACTUS_GROWING_HEIGHT: i32 = 3;
const ATTEMPT_GROW_CACTUS_FLOWER_AGE: i32 = 8;

pub struct CactusBlock;

impl BlockBehaviour for CactusBlock {
    fn entity_inside(
        &self,
        _state: BlockState,
        level: &mut crate::server::level::ServerLevel,
        _pos: BlockPos,
        entity: &mut dyn crate::world::entity::Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        let source = level.damage_sources().cactus();
        entity.hurt_server(level, &source, 1.0);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.can_survive(&*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let above = pos.above();
        if !level.is_empty_block(above) {
            return;
        }
        let mut height = 1;
        let age = state.get(props::AGE_15);
        while level.get_block_state(pos.below_n(height)).is_block(state.block()) {
            height += 1;
            if height == MAX_CACTUS_GROWING_HEIGHT && age == 15 {
                return;
            }
        }
        if age == ATTEMPT_GROW_CACTUS_FLOWER_AGE && self.can_survive(state.block().default_state(), &*level, pos.above()) {
            let chance_to_grow_flower = if height >= 3 { 0.25 } else { 0.1 };
            if level.random().next_double() <= chance_to_grow_flower {
                level.set_block_and_update(above, Block::CACTUS_FLOWER.default_state());
            }
        } else if age == 15 && height < 3 {
            level.set_block_and_update(above, state.block().default_state());
            let above_block = state.set(props::AGE_15, 0);
            level.set_block_flags(pos, above_block, 260);
            level.neighbor_changed_with_state(above_block, above, state.block(), None, false);
        }
        if age < 15 {
            level.set_block_flags(pos, state.set(props::AGE_15, age + 1), 260);
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        for direction in Direction::HORIZONTAL {
            let neighbor = level.get_block_state(pos.relative(direction));
            if neighbor.is_solid() || level.get_fluid_state(pos.relative(direction)).is_tag("lava") {
                return false;
            }
        }
        let below_state = level.get_block_state(pos.below());
        (below_state.is_block(state.block()) || below_state.is_tag("supports_cactus")) && !level.get_block_state(pos.above()).liquid()
    }
}

register_block_class!("CactusBlock", |_| Box::new(CactusBlock));
