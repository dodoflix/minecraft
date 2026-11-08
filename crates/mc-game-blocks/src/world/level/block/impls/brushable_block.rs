//! `net.minecraft.world.level.block.BrushableBlock` (suspicious sand / gravel; extends `BaseEntityBlock`, implements `Fallable`).
use crate::world::block_entity::BrushableBlockEntity;
use mc_core::phys::Vec3;
use mc_data::{GameEvent, SoundEvent};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::falling_block_entity::FallingBlockEntity;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::BrushableData;
use mc_game_core::world::level::block::impls::falling_block;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

/// `TICK_DELAY`.
const TICK_DELAY: i32 = 2;

pub struct BrushableBlock {
    data: BrushableData,
}

impl BrushableBlock {
    /// `new BrushableBlock(turnsInto, brushSound, brushCompletedSound, properties)` of `Blocks.SUSPICIOUS_SAND` / `SUSPICIOUS_GRAVEL`.
    fn new(block: Block) -> Self {
        let (turns_into, sound) = match block.data().name {
            "minecraft:suspicious_sand" => (Block::SAND, SoundEvent::ITEM_BRUSH_BRUSHING_SAND),
            "minecraft:suspicious_gravel" => {
                (Block::GRAVEL, SoundEvent::ITEM_BRUSH_BRUSHING_GRAVEL)
            }
            other => panic!("`{other}` is not a brushable block"),
        };
        Self {
            data: BrushableData {
                turns_into,
                brush_sound: sound,
                brush_completed_sound: sound,
            },
        }
    }
}

impl BlockBehaviour for BrushableBlock {
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        falling_block::on_place(state, level, pos, TICK_DELAY);
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            falling_block::update_shape(state, level, pos, TICK_DELAY)
        }
    );
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        level.with_block_entity(pos, |level, be| {
            if let Some(brushable) = be.as_any_mut().downcast_mut::<BrushableBlockEntity>() {
                brushable.check_reset(level);
            }
        });
        if falling_block::is_free(level.get_block_state(pos.below()))
            && pos.y() >= level.get_min_y()
        {
            FallingBlockEntity::fall_with(level, pos, state, FallingBlockEntity::disable_drop);
        }
    }
    fn on_broken_after_fall(
        &self,
        level: &mut ServerLevel,
        _pos: BlockPos,
        entity: &FallingBlockEntity,
    ) {
        let center: Vec3 = entity.get_bounding_box().get_center();
        LevelAccessor::level_event(
            level,
            None,
            2001,
            BlockPos::containing_position(&center),
            entity.get_block_state().id() as i32,
        );
        level.game_event_from_entity_ref(entity, GameEvent::BLOCK_DESTROY, center);
    }
    fn as_brushable(&self) -> Option<BrushableData> {
        Some(self.data)
    }
}

register_block_class!("BrushableBlock", |block| Box::new(BrushableBlock::new(
    block
)));
